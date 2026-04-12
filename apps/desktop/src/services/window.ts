import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export async function showEditor(): Promise<void> {
  const editor = await WebviewWindow.getByLabel("editor");
  if (editor) {
    await editor.show();
    await editor.setFocus();
  }
}

export async function hideEditor(): Promise<void> {
  const editor = await WebviewWindow.getByLabel("editor");
  if (editor) {
    await editor.hide();
  }
}
