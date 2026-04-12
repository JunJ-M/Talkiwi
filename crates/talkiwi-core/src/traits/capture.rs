use crate::event::{ActionEvent, ActionType};
use tokio::sync::mpsc;

/// Permission status for an action capture module.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    NotRequired,
}

/// Action capture trait — implemented by selection, screenshot, clipboard, etc.
pub trait ActionCapture: Send + Sync {
    fn id(&self) -> &str;
    fn action_types(&self) -> &[ActionType];
    fn start(&mut self, tx: mpsc::Sender<ActionEvent>) -> anyhow::Result<()>;
    fn stop(&mut self) -> anyhow::Result<()>;
    fn check_permission(&self) -> PermissionStatus;
}
