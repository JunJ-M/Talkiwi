use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait};

use talkiwi_core::preview::AudioInputInfo;

#[derive(Debug, Clone, Default)]
pub struct SelectedAudioInput {
    inner: Arc<Mutex<Option<String>>>,
}

impl SelectedAudioInput {
    pub fn get(&self) -> Option<String> {
        self.inner.lock().ok().and_then(|guard| guard.clone())
    }

    pub fn set(&self, selected: Option<String>) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = selected;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AudioInputManager {
    selected: SelectedAudioInput,
}

impl AudioInputManager {
    pub fn new(selected: SelectedAudioInput) -> Self {
        Self { selected }
    }

    pub fn selected(&self) -> SelectedAudioInput {
        self.selected.clone()
    }

    pub fn get_selected_input(&self) -> Option<String> {
        self.selected.get()
    }

    pub fn list_inputs(&self) -> anyhow::Result<Vec<AudioInputInfo>> {
        let host = cpal::default_host();
        let default_name = host
            .default_input_device()
            .and_then(|device| device.name().ok());

        let mut inputs = Vec::new();
        for device in host.input_devices()? {
            let name = device
                .name()
                .unwrap_or_else(|_| "Unknown Input".to_string());
            let mut sample_rates = Vec::new();
            let mut channels = Vec::new();
            if let Ok(configs) = device.supported_input_configs() {
                for config in configs {
                    sample_rates.push(config.min_sample_rate().0);
                    sample_rates.push(config.max_sample_rate().0);
                    channels.push(config.channels());
                }
            }
            sample_rates.sort_unstable();
            sample_rates.dedup();
            channels.sort_unstable();
            channels.dedup();
            inputs.push(AudioInputInfo {
                id: name.clone(),
                name: name.clone(),
                is_default: default_name.as_deref() == Some(name.as_str()),
                sample_rates,
                channels,
            });
        }
        Ok(inputs)
    }

    pub fn set_selected_input(&self, id_or_name: &str) -> anyhow::Result<Option<AudioInputInfo>> {
        let inputs = self.list_inputs()?;
        let selected = inputs
            .into_iter()
            .find(|input| input.id == id_or_name || input.name == id_or_name)
            .ok_or_else(|| anyhow::anyhow!("audio input '{}' not found", id_or_name))?;
        self.selected.set(Some(selected.id.clone()));
        Ok(Some(selected))
    }

    pub fn resolve_selected_input(&self) -> anyhow::Result<Option<AudioInputInfo>> {
        let inputs = self.list_inputs()?;
        Ok(resolve_input_from_available(
            &inputs,
            self.selected.get().as_deref(),
        ))
    }
}

fn resolve_input_from_available(
    inputs: &[AudioInputInfo],
    selected: Option<&str>,
) -> Option<AudioInputInfo> {
    if let Some(selected) = selected {
        if let Some(input) = inputs
            .iter()
            .find(|input| input.id == selected || input.name == selected)
        {
            return Some(input.clone());
        }
    }

    inputs
        .iter()
        .find(|input| input.is_default)
        .cloned()
        .or_else(|| inputs.first().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: &str, is_default: bool) -> AudioInputInfo {
        AudioInputInfo {
            id: id.to_string(),
            name: id.to_string(),
            is_default,
            sample_rates: vec![44_100],
            channels: vec![1],
        }
    }

    #[test]
    fn resolve_input_prefers_matching_selected_id() {
        let inputs = vec![input("Built-in Mic", true), input("USB Mic", false)];

        let resolved = resolve_input_from_available(&inputs, Some("USB Mic"))
            .expect("expected selected input");

        assert_eq!(resolved.id, "USB Mic");
    }

    #[test]
    fn resolve_input_falls_back_to_default_device() {
        let inputs = vec![input("Built-in Mic", true), input("USB Mic", false)];

        let resolved =
            resolve_input_from_available(&inputs, None).expect("expected default input");

        assert_eq!(resolved.id, "Built-in Mic");
    }

    #[test]
    fn resolve_input_falls_back_to_first_when_default_missing() {
        let inputs = vec![input("Mic A", false), input("Mic B", false)];

        let resolved =
            resolve_input_from_available(&inputs, None).expect("expected first input");

        assert_eq!(resolved.id, "Mic A");
    }
}
