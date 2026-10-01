use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::events::{Event, Reporter};
use crate::shared::soundcloud::client::SoundCloudClient;

pub struct CountingAllocator;

pub static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Debug, Clone)]
pub struct RawRequest {
    pub path: String,
    pub query: String,
    pub range_from: Option<u64>,
}

pub enum RawResponse {
    Full {
        status: u16,
        content_type: &'static str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
    },
    Truncated {
        declared: usize,
        body: Vec<u8>,
    },
    Delayed(Duration, Box<RawResponse>),
}

impl RawResponse {
    pub fn json(value: &serde_json::Value) -> Self {
        Self::Full {
            status: 200,
            content_type: "application/json",
            headers: Vec::new(),
            body: value.to_string().into_bytes(),
        }
    }

    pub fn audio(body: Vec<u8>) -> Self {
        Self::Full {
            status: 200,
            content_type: "audio/mpeg",
            headers: Vec::new(),
            body,
        }
    }

    pub fn partial(full: &[u8], from: u64) -> Self {
        let start = usize::try_from(from).expect("offset").min(full.len());
        Self::Full {
            status: 206,
            content_type: "audio/mpeg",
            headers: vec![(
                "Content-Range",
                format!("bytes {start}-{}/{}", full.len() - 1, full.len()),
            )],
            body: full[start..].to_vec(),
        }
    }
}

type Handler = Arc<dyn Fn(&RawRequest) -> RawResponse + Send + Sync>;

#[derive(Default)]
pub struct RawStats {
    pub requests: Mutex<Vec<RawRequest>>,
    inflight: AtomicUsize,
    pub max_inflight: AtomicUsize,
}

impl RawStats {
    pub fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .expect("lock")
            .iter()
            .filter(|r| r.path == path)
            .count()
    }
}

pub struct RawServer {
    pub base: String,
    pub stats: Arc<RawStats>,
}

impl RawServer {
    pub async fn start(
        handler: impl Fn(&RawRequest) -> RawResponse + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("porta local");
        let base = format!("http://{}", listener.local_addr().expect("endereco"));
        let stats = Arc::new(RawStats::default());
        let handler: Handler = Arc::new(handler);
        let shared = Arc::clone(&stats);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(serve(socket, Arc::clone(&handler), Arc::clone(&shared)));
            }
        });
        Self { base, stats }
    }

    pub fn client(&self) -> Arc<SoundCloudClient> {
        Arc::new(SoundCloudClient::for_tests(&self.base, Some(ID.into())))
    }
}

async fn serve(mut socket: tokio::net::TcpStream, handler: Handler, stats: Arc<RawStats>) {
    let mut raw = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
        match socket.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(read) => raw.extend_from_slice(&buffer[..read]),
        }
        if raw.len() > 64 * 1024 {
            return;
        }
    }
    let head = String::from_utf8_lossy(&raw);
    let target = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let range_from = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.eq_ignore_ascii_case("range") {
            return None;
        }
        value
            .trim()
            .strip_prefix("bytes=")?
            .trim_end_matches('-')
            .parse()
            .ok()
    });
    let request = RawRequest {
        path: path.to_owned(),
        query: query.to_owned(),
        range_from,
    };
    stats.requests.lock().expect("lock").push(request.clone());
    let now = stats.inflight.fetch_add(1, Ordering::SeqCst) + 1;
    stats.max_inflight.fetch_max(now, Ordering::SeqCst);
    let mut response = handler(&request);
    while let RawResponse::Delayed(delay, inner) = response {
        tokio::time::sleep(delay).await;
        response = *inner;
    }
    let _ = write_response(&mut socket, response).await;
    stats.inflight.fetch_sub(1, Ordering::SeqCst);
}

async fn write_response(
    socket: &mut tokio::net::TcpStream,
    response: RawResponse,
) -> std::io::Result<()> {
    match response {
        RawResponse::Full {
            status,
            content_type,
            headers,
            body,
        } => {
            let mut head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                let _ = write!(head, "{name}: {value}\r\n");
            }
            head.push_str("\r\n");
            socket.write_all(head.as_bytes()).await?;
            socket.write_all(&body).await?;
        }
        RawResponse::Truncated { declared, body } => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n"
            );
            socket.write_all(head.as_bytes()).await?;
            socket.write_all(&body).await?;
        }
        RawResponse::Delayed(..) => unreachable!("atrasos resolvidos antes"),
    }
    socket.flush().await?;
    socket.shutdown().await
}

pub const ID: &str = "abcdefghijklmnopqrstuvwxyz012345";

pub fn mp3_frames(count: usize) -> Vec<u8> {
    let mut frame = vec![0_u8; 417];
    frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
    frame.repeat(count)
}

pub async fn mount_media(server: &MockServer) {
    let uri = server.uri();
    Mock::given(path("/progressive"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"url": format!("{uri}/audio.mp3")})),
        )
        .mount(server)
        .await;
    Mock::given(path("/hls"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"url": format!("{uri}/playlist.m3u8")})),
        )
        .mount(server)
        .await;
    Mock::given(path("/audio.mp3"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "audio/mpeg")
                .set_body_bytes(mp3_frames(200)),
        )
        .mount(server)
        .await;
    let manifest = "#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXTINF:2.0,\nseg0.mp3\n#EXTINF:2.0,\nseg1.mp3\n#EXTINF:2.0,\nseg2.mp3\n#EXT-X-ENDLIST\n";
    Mock::given(path("/playlist.m3u8"))
        .respond_with(ResponseTemplate::new(200).set_body_string(manifest))
        .mount(server)
        .await;
    for index in 0..3 {
        Mock::given(path(format!("/seg{index}.mp3")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(mp3_frames(80)))
            .mount(server)
            .await;
    }
}

pub fn track(id: i64, title: &str, protocol: &str, base: &str) -> serde_json::Value {
    json!({"id": id, "title": title, "user": {"username": "Perseus"}, "created_at": "2024-05-01T00:00:00Z",
           "duration": 180_000,
           "media": {"transcodings": [{"url": format!("{base}/{protocol}"), "format": {"protocol": protocol, "mime_type": "audio/mpeg"}}]}})
}

pub fn collecting_reporter() -> (Reporter, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    (
        Reporter::new(
            "test",
            Arc::new(move |event: &Event| sink.lock().expect("lock").push(event.clone())),
        ),
        events,
    )
}

pub fn client(server: &MockServer) -> Arc<SoundCloudClient> {
    Arc::new(SoundCloudClient::for_tests(&server.uri(), Some(ID.into())))
}
