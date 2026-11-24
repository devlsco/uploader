export const LANGUAGES = [
  { value: "auto", label: "Auto detect" },
  { value: "plaintext", label: "Plain text" },
  { value: "bash", label: "Bash" },
  { value: "c", label: "C" },
  { value: "cpp", label: "C++" },
  { value: "csharp", label: "C#" },
  { value: "css", label: "CSS" },
  { value: "diff", label: "Diff" },
  { value: "go", label: "Go" },
  { value: "xml", label: "HTML / XML" },
  { value: "ini", label: "INI / TOML" },
  { value: "java", label: "Java" },
  { value: "javascript", label: "JavaScript" },
  { value: "json", label: "JSON" },
  { value: "kotlin", label: "Kotlin" },
  { value: "lua", label: "Lua" },
  { value: "markdown", label: "Markdown" },
  { value: "php", label: "PHP" },
  { value: "python", label: "Python" },
  { value: "ruby", label: "Ruby" },
  { value: "rust", label: "Rust" },
  { value: "sql", label: "SQL" },
  { value: "swift", label: "Swift" },
  { value: "typescript", label: "TypeScript" },
  { value: "yaml", label: "YAML" },
];

/** highlight.js language from a file name. */
export function languageOf(name: string, mime?: string): string | undefined {
  const ext = name.toLowerCase().split(".").pop() ?? "";
  const map: Record<string, string> = {
    js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "javascript", ts: "typescript", tsx: "typescript", py: "python", rs: "rust", go: "go",
    java: "java", kt: "kotlin", c: "c", h: "c", cpp: "cpp", cc: "cpp", hpp: "cpp", cs: "csharp", css: "css", scss: "scss", html: "xml", htm: "xml", xml: "xml",
    svg: "xml", json: "json", yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", conf: "ini", sh: "bash", bash: "bash", zsh: "bash", sql: "sql", md: "markdown",
    php: "php", rb: "ruby", swift: "swift", lua: "lua", diff: "diff", patch: "diff", log: "plaintext", txt: "plaintext",
  };
  if (map[ext]) return map[ext];
  if (mime === "application/json") return "json";
  return undefined;
}
