pub mod click;
pub mod clipboard;
pub mod file;
pub mod focus;
pub mod page;
pub mod screenshot;
pub mod selection;

pub use click::ClickCapture;
pub use clipboard::ClipboardCapture;
pub use file::FileCapture;
pub use focus::FocusCapture;
pub use page::PageCapture;
pub use screenshot::ScreenshotCapture;
pub use selection::SelectionCapture;
pub use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};
