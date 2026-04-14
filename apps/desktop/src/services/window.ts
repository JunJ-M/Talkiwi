import { emitTo } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isTauriRuntime } from "./runtime";

async function getEditorWindow() {
  if (!isTauriRuntime()) {
    return null;
  }
  return WebviewWindow.getByLabel("editor");
}

export async function showEditor(): Promise<void> {
  const editor = await getEditorWindow();
  if (editor) {
    await editor.show();
    await editor.setFocus();
  }
}

export async function hideEditor(): Promise<void> {
  const editor = await getEditorWindow();
  if (editor) {
    await editor.hide();
  }
}

export async function showSettings(): Promise<void> {
  const editor = await getEditorWindow();
  if (editor) {
    await editor.show();
    await emitTo("editor", "talkiwi://open-settings");
    await editor.setFocus();
  }
}

export async function showHistory(): Promise<void> {
  const editor = await getEditorWindow();
  if (editor) {
    await editor.show();
    await emitTo("editor", "talkiwi://open-history");
    await editor.setFocus();
  }
}
