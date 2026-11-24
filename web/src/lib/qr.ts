export type QrOptions = {
  size: number;
  margin: number;
  dark: string;
  light: string;
  ecc: "L" | "M" | "Q" | "H";
};

export const QR_DEFAULTS: QrOptions = { size: 512, margin: 2, dark: "#000000", light: "#ffffff", ecc: "M" };

const lib = () => import("qrcode").then((m) => m.default ?? m);

export async function qrSvg(text: string, o: QrOptions = QR_DEFAULTS): Promise<string> {
  const q = await lib();
  return q.toString(text, { type: "svg", margin: o.margin, errorCorrectionLevel: o.ecc, color: { dark: o.dark, light: o.light }, width: o.size });
}

export async function qrPng(text: string, o: QrOptions = QR_DEFAULTS): Promise<string> {
  const q = await lib();
  return q.toDataURL(text, { margin: o.margin, errorCorrectionLevel: o.ecc, color: { dark: o.dark, light: o.light }, width: o.size });
}

/** WIFI:T:WPA;S:name;P:password;; with the special characters escaped */
export function wifiText(ssid: string, password: string, security: "WPA" | "WEP" | "nopass", hidden: boolean): string {
  const esc = (s: string) => s.replace(/([\;,":])/g, "\\$1");
  return `WIFI:T:${security};S:${esc(ssid)};${security === "nopass" ? "" : `P:${esc(password)};`}${hidden ? "H:true;" : ""};`;
}

export function downloadUrl(url: string, name: string) {
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
}

export async function copyPng(dataUrl: string): Promise<void> {
  const blob = await (await fetch(dataUrl)).blob();
  await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
}
