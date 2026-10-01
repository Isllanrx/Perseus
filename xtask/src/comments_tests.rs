use super::*;
use crate::lexers::{Language, is_directive, lex};

fn texts(language: Language, src: &str) -> Vec<String> {
    lex(language, src)
        .expect("lexes")
        .into_iter()
        .map(|c| src[c.start..c.end].to_string())
        .collect()
}

fn clean(language: Language, src: &str) -> String {
    let removable: Vec<_> = lex(language, src)
        .expect("lexes")
        .into_iter()
        .filter(|c| !is_directive(language, src, *c))
        .collect();
    let stripped = strip_comments(src, &removable);
    verify(language, src, &stripped).expect("verified");
    stripped
}

#[test]
fn rust_literals_that_look_like_comments_are_code() {
    let src = concat!(
        "let url = \"https://x\"; // trailing\n",
        "let raw = r#\"a // b /* c\"#;\n",
        "let bytes = b\"//\";\n",
        "let quote = '\"'; let tick = '\\''; let slash = '/';\n",
        "fn f<'a>(x: &'a str) -> &'a str { x }\n",
        "/* outer /* nested */ still */\n",
        "/// doc\n",
    );
    assert_eq!(
        texts(Language::Rust, src),
        ["// trailing", "/* outer /* nested */ still */", "/// doc"]
    );
}

#[test]
fn rust_cleaning_keeps_code_and_attributes() {
    let src = concat!(
        "//! Modulo.\n",
        "\n",
        "/// Explica f.\n",
        "#[allow(clippy::x, reason = \"// nao e comentario\")]\n",
        "fn f() {\n",
        "    // passo um\n",
        "    let a = 1; // por que\n",
        "}\n",
    );
    assert_eq!(
        clean(Language::Rust, src),
        "#[allow(clippy::x, reason = \"// nao e comentario\")]\nfn f() {\n    let a = 1;\n}\n"
    );
}

#[test]
fn script_regex_templates_and_division_are_not_comments() {
    let src = concat!(
        "const re = /\\/\\/[/*]/g;\n",
        "const t = `a // ${ \"/*\" + `in ${x // y\n} ` } b`;\n",
        "const d = a / b; // half\n",
        "if (re.test(s)) return /x/; /* note */\n",
    );
    assert_eq!(
        texts(Language::Script, src),
        ["// y", "// half", "/* note */"]
    );
}

#[test]
fn script_keeps_one_line_tool_directives_and_the_shebang() {
    let src = concat!(
        "#!/usr/bin/env node\n",
        "/* eslint-disable some-rule -- motivo */\n",
        "/// <reference types=\"vite/client\" />\n",
        "// explicacao\n",
        "// eslint-disable-next-line other-rule\n",
        "throw { kind };\n",
    );
    assert_eq!(
        clean(Language::Script, src),
        concat!(
            "#!/usr/bin/env node\n",
            "/* eslint-disable some-rule -- motivo */\n",
            "/// <reference types=\"vite/client\" />\n",
            "// eslint-disable-next-line other-rule\n",
            "throw { kind };\n",
        )
    );
}

#[test]
fn jsx_text_is_content_not_strings_or_comments() {
    let src = concat!(
        "export const A = () => (\n",
        "  <p className=\"x\" title='it\"s'>\n",
        "    Don't open https://example.com // here /* or here */\n",
        "    {/* explica o botao */}\n",
        "    {cond ? <b>{\"//\"}</b> : null /* fim */}\n",
        "    <>fragment's text</>\n",
        "  </p>\n",
        ");\n",
    );
    assert_eq!(
        texts(Language::Tsx, src),
        ["{/* explica o botao */}", "/* fim */"]
    );
    assert_eq!(
        clean(Language::Tsx, src),
        concat!(
            "export const A = () => (\n",
            "  <p className=\"x\" title='it\"s'>\n",
            "    Don't open https://example.com // here /* or here */\n",
            "    {cond ? <b>{\"//\"}</b> : null }\n",
            "    <>fragment's text</>\n",
            "  </p>\n",
            ");\n",
        )
    );
}

#[test]
fn tsx_generics_and_comparisons_are_not_jsx() {
    let src = concat!(
        "const pick = <K extends keyof O>(key: K): O[K] => o[key]; // escolhe\n",
        "const pair = <A, B>(a: A, b: B) => [a, b];\n",
        "const ref = useRef<HTMLDivElement>(null);\n",
        "if (a < b && c > d) render(<Box />); // desenha\n",
    );
    assert_eq!(texts(Language::Tsx, src), ["// escolhe", "// desenha"]);
}

#[test]
fn html_comments_and_embedded_css_and_script_are_found() {
    let src = concat!(
        "<!doctype html>\n",
        "<!-- page -->\n",
        "<a title=\"<!-- not -->\" href=\"x\">link</a>\n",
        "<style>\n  /* palette */\n  a::after { content: \"/* no */\"; }\n</style>\n",
        "<script>\n  const u = \"//\"; // note\n</script>\n",
    );
    assert_eq!(
        texts(Language::Html, src),
        ["<!-- page -->", "/* palette */", "// note"]
    );
    assert_eq!(
        clean(Language::Html, src),
        concat!(
            "<!doctype html>\n",
            "<a title=\"<!-- not -->\" href=\"x\">link</a>\n",
            "<style>\n  a::after { content: \"/* no */\"; }\n</style>\n",
            "<script>\n  const u = \"//\";\n</script>\n",
        )
    );
}

#[test]
fn toml_hashes_inside_strings_are_values() {
    let src = concat!(
        "# header\n",
        "\n",
        "key = \"a # b\" # why\n",
        "lit = 'c # d'\n",
        "multi = '''\n# kept\n'''\n",
        "basic = \"\"\"\n# kept \\\"\"\" too\n\"\"\"\n",
    );
    assert_eq!(texts(Language::Toml, src), ["# header", "# why"]);
    assert_eq!(
        clean(Language::Toml, src),
        concat!(
            "key = \"a # b\"\n",
            "lit = 'c # d'\n",
            "multi = '''\n# kept\n'''\n",
            "basic = \"\"\"\n# kept \\\"\"\" too\n\"\"\"\n",
        )
    );
}

#[test]
fn yaml_keeps_directives_and_reads_shell_comments_inside_run() {
    let src = concat!(
        "name: CI\n",
        "\n",
        "# explains the workflow\n",
        "\n",
        "on:\n",
        "  workflow_run: # zizmor: ignore[dangerous-triggers] reason\n",
        "jobs:\n",
        "  a:\n",
        "    runs-on: ubuntu-latest\n",
        "    steps:\n",
        "      - uses: actions/checkout@0123456789abcdef0123456789abcdef01234567 # v7.0.1\n",
        "      - name: 'it''s # fine'\n",
        "        run: |\n",
        "          # why this step\n",
        "          echo \"PR #$PR\" ${#x} $#\n",
        "          python3 - << 'EOF'\n",
        "          # python keeps its own text\n",
        "          EOF\n",
        "          echo done # trailing\n",
        "      - name: notes\n",
        "        with:\n",
        "          body: |\n",
        "            # A markdown heading\n",
    );
    assert_eq!(
        texts(Language::Yaml, src),
        [
            "# explains the workflow",
            "# zizmor: ignore[dangerous-triggers] reason",
            "# v7.0.1",
            "# why this step",
            "# trailing"
        ]
    );
    let cleaned = clean(Language::Yaml, src);
    assert!(cleaned.starts_with("name: CI\n\non:\n"));
    assert!(cleaned.contains("workflow_run: # zizmor: ignore[dangerous-triggers] reason\n"));
    assert!(cleaned.contains("@0123456789abcdef0123456789abcdef01234567 # v7.0.1\n"));
    assert!(cleaned.contains("        run: |\n          echo \"PR #$PR\""));
    assert!(cleaned.contains("          # python keeps its own text\n"));
    assert!(cleaned.contains("          echo done\n"));
    assert!(cleaned.contains("            # A markdown heading\n"));
}

#[test]
fn yaml_pwsh_steps_use_powershell_quoting() {
    let src = concat!(
        "jobs:\n",
        "  w:\n",
        "    runs-on: windows-latest\n",
        "    steps:\n",
        "      - run: |\n",
        "          $p = \"C:\\dir `\" # inside a pwsh string\"\n",
        "          <# block #>\n",
        "          Write-Host `#literal\n",
        "      - shell: bash\n",
        "        run: |\n",
        "          echo \"a\\\" # still inside\"\n",
    );
    assert_eq!(texts(Language::Yaml, src), ["<# block #>"]);
}

#[test]
fn powershell_scripts_drop_help_and_comments_but_keep_requires() {
    let src = concat!(
        "#Requires -Version 7\n",
        "<#\n",
        ".SYNOPSIS\n",
        "  Faz algo.\n",
        "#>\n",
        "param([switch]$Full)\n",
        "# explicacao\n",
        "$tag = \"Perseus #1\" # fim\n",
        "$here = @'\n",
        "# texto do here-string\n",
        "'@\n",
    );
    assert_eq!(
        clean(Language::PowerShell, src),
        concat!(
            "#Requires -Version 7\n",
            "param([switch]$Full)\n",
            "$tag = \"Perseus #1\"\n",
            "$here = @'\n",
            "# texto do here-string\n",
            "'@\n",
        )
    );
}

#[test]
fn doc_comments_read_by_clap_are_runtime_text_not_comments() {
    let removable = |src: &str| lex(Language::Rust, src).expect("lexes");
    let clap = concat!(
        "#[derive(Clone, clap::Parser)]\n",
        "struct Args {\n",
        "    /// Aparece no --help\n",
        "    url: String,\n",
        "}\n",
    );
    assert!(runtime_docs(clap, &removable(clap)));
    let plain = "// so explica\n#[derive(Parser)]\nstruct Args { url: String }\n";
    assert!(!runtime_docs(plain, &removable(plain)));
    let serde = "/// Documenta\n#[derive(Debug, Serialize)]\nstruct Out { id: i64 }\n";
    assert!(!runtime_docs(serde, &removable(serde)));
}

#[test]
fn a_changed_character_of_code_is_refused() {
    let src = "let a = 1; // why\n";
    assert!(verify(Language::Rust, src, "let a = 2;\n").is_err());
    assert!(verify(Language::Rust, src, "let a = 1; // why\n").is_err());
    assert!(verify(Language::Rust, src, "let a = 1;\n").is_ok());
}

#[test]
fn an_unterminated_literal_is_an_error_not_a_guess() {
    assert!(lex(Language::Rust, "let s = \"open;\n").is_err());
    assert!(lex(Language::Script, "const t = `open ${a;\n").is_err());
    assert!(lex(Language::Html, "<!-- open\n").is_err());
    assert!(lex(Language::Tsx, "const a = <div>open\n").is_err());
}

#[test]
fn only_code_files_are_in_scope() {
    assert_eq!(Language::of("a/b.rs"), Some(Language::Rust));
    assert_eq!(Language::of("web/src/App.tsx"), Some(Language::Tsx));
    assert_eq!(
        Language::of("web/src/vite-env.d.ts"),
        Some(Language::Script)
    );
    assert_eq!(
        Language::of("scripts/test-all.ps1"),
        Some(Language::PowerShell)
    );
    assert_eq!(
        Language::of(".github/workflows/ci.yml"),
        Some(Language::Yaml)
    );
    assert_eq!(Language::of("README.md"), None);
    assert_eq!(Language::of("contracts/job-in.json"), None);
    assert_eq!(Language::of(".gitignore"), None);
}
