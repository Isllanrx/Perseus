#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";

const RULES = [
  { name: "chave privada", pattern: /-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----/ },
  { name: "token GitHub", pattern: /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b|\bgithub_pat_[A-Za-z0-9_]{60,}\b/ },
  { name: "chave AWS", pattern: /\bAKIA[0-9A-Z]{16}\b/ },
  { name: "chave Google", pattern: /\bAIza[0-9A-Za-z_-]{35}\b/ },
  { name: "token Slack", pattern: /\bxox[abprs]-[A-Za-z0-9-]{10,}\b/ },
  { name: "chave Anthropic/OpenAI", pattern: /\bsk-(?:ant-)?[A-Za-z0-9_-]{32,}\b/ },
  { name: "client_id do SoundCloud", pattern: /client_id["'=:\s]+([A-Za-z0-9]{32})\b/ },
  { name: "senha em atribuicao", pattern: /\b(?:password|passwd|senha)\s*[:=]\s*["'][^"'\s]{8,}["']/i },
];
const ALLOWED = new Set(["abcdefghijklmnopqrstuvwxyz012345"]);
const SKIP = /\.(png|jpe?g|ico|icns|gif|webp|woff2?|ttf|otf|mp3|m4a|opus|zip|exe|lock)$|package-lock\.json$|Cargo\.lock$/i;

const files = execFileSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], { encoding: "utf8" })
  .split("\0")
  .filter((file) => file && !SKIP.test(file));

const findings = [];
for (const file of files) {
  let content;
  try {
    if (statSync(file).size > 2_000_000) continue;
    content = readFileSync(file, "utf8");
  } catch {
    continue;
  }
  content.split(/\r?\n/).forEach((line, index) => {
    for (const rule of RULES) {
      const match = rule.pattern.exec(line);
      if (!match) continue;
      const secret = match[1] ?? match[0];
      if (ALLOWED.has(secret)) continue;
      findings.push(`${file}:${index + 1}  ${rule.name}  ${secret.slice(0, 6)}...`);
    }
  });
}

if (findings.length > 0) {
  console.error(`segredos encontrados (${findings.length}):\n${findings.join("\n")}`);
  process.exit(1);
}
console.log(`sem segredos em ${files.length} arquivos versionados`);
