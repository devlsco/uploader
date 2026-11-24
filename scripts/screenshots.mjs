import { mkdirSync } from "node:fs";
import { chromium } from "playwright";

const BASE = "http://localhost:3000";
const OUT = "docs";
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 1360, height: 860 }, deviceScaleFactor: 2, colorScheme: "dark" });
const page = await ctx.newPage();
const api = (path, body) =>
  page.evaluate(
    async ([path, body]) => {
      const r = await fetch(`/api${path}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      return r.status === 204 ? {} : r.json();
    },
    [path, body],
  );

await page.goto(`${BASE}/setup`);
await page.waitForLoadState("networkidle");
await api("/setup", { code: process.env.SETUP_CODE ?? "", username: "lennox", password: "screenshot-demo" });

const scenes = [
  ["sunset.jpg", "linear-gradient(180deg,#1e1b4b 0%,#7c2d12 45%,#f59e0b 70%,#fde68a 100%)", "radial-gradient(circle at 50% 72%,#fff7 0,#fff0 18%)"],
  ["ocean.jpg", "linear-gradient(180deg,#e0f2fe 0%,#7dd3fc 40%,#0369a1 70%,#082f49 100%)", "radial-gradient(ellipse at 30% 30%,#fff8 0,#fff0 30%)"],
  ["forest.jpg", "linear-gradient(160deg,#052e16 0%,#166534 45%,#86efac 100%)", "radial-gradient(circle at 75% 20%,#fef9c3aa 0,#fff0 25%)"],
  ["mountains.jpg", "linear-gradient(180deg,#c7d2fe 0%,#818cf8 40%,#312e81 100%)", "linear-gradient(160deg,#0000 55%,#1e1b4b 55.2%,#1e1b4b 100%),linear-gradient(200deg,#0000 60%,#312e81 60.2%)"],
  ["desert.jpg", "linear-gradient(180deg,#fed7aa 0%,#fb923c 50%,#9a3412 100%)", "radial-gradient(ellipse at 60% 90%,#7c2d12 0,#7c2d1200 50%)"],
  ["night.jpg", "linear-gradient(180deg,#020617 0%,#1e1b4b 60%,#4c1d95 100%)", "radial-gradient(circle at 80% 25%,#fef3c7 0,#fef3c700 6%)"],
];
const shot = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
const ids = [];
for (const [name, bg, glow] of scenes) {
  await shot.setContent(`<body style="margin:0;width:1600px;height:1000px;background:${glow},${bg}"></body>`);
  const jpg = await shot.screenshot({ type: "jpeg", quality: 88 });
  const item = await page.evaluate(
    async ([name, b64]) => {
      const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const fd = new FormData();
      fd.append("file", new Blob([bin], { type: "image/jpeg" }), name);
      return (await fetch("/api/upload", { method: "POST", body: fd })).json();
    },
    [name, jpg.toString("base64")],
  );
  ids.push(item.id);
}
const code = `use axum::{Router, routing::get};

/// One small binary: the API, the web app and your files.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api", api::router());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
`;
const paste = await api("/paste", { content: code, title: "main.rs", language: "rust" });
await api("/links", { url: "https://github.com/devlsco/uploader", slug: "repo" });
const album = await api("/albums", { title: "Summer 2026", description: "Photos from the trip" });
await page.evaluate(async ([ids, album]) => fetch("/api/items/album", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ids, album_id: album }) }), [ids.slice(0, 5), album.id]);
await new Promise((r) => setTimeout(r, 2500)); // thumbnails

async function capture(path, file, prepare) {
  await page.goto(`${BASE}${path}`);
  await page.waitForLoadState("networkidle");
  if (prepare) await prepare();
  await page.waitForTimeout(600);
  await page.screenshot({ path: `${OUT}/${file}.png` });
}
await page.goto(`${BASE}/`);
await page.getByRole("dialog", { name: "Welcome tour" }).waitFor();
await page.waitForTimeout(500);
await page.screenshot({ path: `${OUT}/tour-welcome.png` });
await page.getByRole("button", { name: "Show me around" }).click();
await page.getByText("Waiting for your first upload").waitFor();
await page.waitForTimeout(700);
await page.screenshot({ path: `${OUT}/tour-upload.png` });
const [picker] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Choose a file" }).click()]);
if (!picker) throw new Error("Choose a file did not open the file picker");
const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.locator('[data-tour="dropzone"] > button').click()]);
await chooser.setFiles({ name: "hello.txt", mimeType: "text/plain", buffer: Buffer.from("Hello from the tour!\n") });
await page.getByText("Uploaded! The link is copied.").waitFor({ timeout: 15000 });
await page.waitForTimeout(500);
await page.screenshot({ path: `${OUT}/tour-uploaded.png` });
for (const name of ["options", "tabs", "files", "settings"]) {
  await page.getByRole("button", { name: /^(Next|Finish)/ }).click();
  await page.waitForTimeout(700);
  await page.screenshot({ path: `${OUT}/tour-${name}.png` });
}
await page.getByRole("button", { name: "Finish" }).click();
await page.getByText("You're all set!").waitFor();
await page.waitForTimeout(800);
await page.screenshot({ path: `${OUT}/tour-done.png` });
await page.getByRole("button", { name: "Start uploading" }).click();
await page.getByRole("dialog", { name: "Welcome tour" }).waitFor({ state: "detached" });
const me = await page.evaluate(async () => (await fetch("/api/me")).json());
if (!me.onboarded) throw new Error("the tour did not save that it was seen");
if (!me.owner) throw new Error("the setup account is not the owner");

await capture("/", "upload");
await capture("/files", "files");
await capture(`/${ids[0]}`, "viewer");
await capture(`/${paste.id}`, "code");
await capture(`/a/${album.id}`, "album");
await capture("/qr", "qr", async () => page.fill("textarea", "https://github.com/devlsco/uploader"));
await capture("/settings", "settings");
await ctx.close();

const light = await browser.newContext({ viewport: { width: 1360, height: 860 }, deviceScaleFactor: 2, colorScheme: "light" });
const lp = await light.newPage();
await lp.goto(`${BASE}/login`);
await lp.waitForLoadState("networkidle");
await lp.screenshot({ path: `${OUT}/login.png` });
await browser.close();
console.log("screenshots done");
