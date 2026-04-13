/** Chinese (zh) UI strings — single source of truth for all user-facing text. */
export const zh = {
  // Common
  retry: "重试",
  loading: "加载中",
  save: "保存配置",
  saving: "保存中...",
  close: "关闭",
  copyFailed: "复制失败",

  // Error boundary
  unknownError: "未知错误",

  // Record / Editor
  regenerate: "重新生成",
  regenerating: "生成中...",

  // Settings
  initialPromptPlaceholder: "用于提升中文口语、术语和代码符号的转写稳定性",
  modelFound: "已找到模型",
  modelMissing: "模型缺失",

  // Permissions
  checkingPermissions: "检查权限",

  // History
  loadingHistory: "加载历史记录",
  loadingSessionDetail: "加载会话详情",

  // Action types (for marble diagram labels)
  actionTypes: {
    "selection.text": "选中文字",
    screenshot: "截图",
    "clipboard.change": "剪贴板",
    "page.current": "当前页面",
    "click.link": "链接",
    "file.attach": "附件",
  } as Record<string, string>,
} as const;
