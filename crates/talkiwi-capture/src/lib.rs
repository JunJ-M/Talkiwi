pub mod clipboard;
pub mod file;
pub mod page;
pub mod screenshot;
pub mod selection;

pub use clipboard::ClipboardCapture;
pub use file::FileCapture;
pub use page::PageCapture;
pub use screenshot::ScreenshotCapture;
pub use selection::SelectionCapture;
pub use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};
