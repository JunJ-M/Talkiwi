//! WAV file writer for persisting audio recordings.
//!
//! Writes PCM 16-bit, 16kHz, mono WAV files. Hand-written header
//! avoids pulling in a new dependency (hound is dev-only).

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Writes raw f32 audio samples to a WAV file (PCM 16-bit, 16kHz, mono).
pub struct WavWriter {
    writer: BufWriter<File>,
    data_bytes_written: u32,
    path: PathBuf,
}

const SAMPLE_RATE: u32 = 16_000;
const BITS_PER_SAMPLE: u16 = 16;
const NUM_CHANNELS: u16 = 1;
const BYTES_PER_SAMPLE: u16 = BITS_PER_SAMPLE / 8;

impl WavWriter {
    /// Create a new WAV writer at the given path.
    ///
    /// Writes a placeholder header that will be patched by `finalize()`.
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);

        // Write placeholder WAV header (44 bytes)
        Self::write_header(&mut writer, 0)?;

        Ok(Self {
            writer,
            data_bytes_written: 0,
            path,
        })
    }

    /// Append f32 samples, converting to i16 PCM.
    pub fn write_chunk(&mut self, samples: &[f32]) -> anyhow::Result<()> {
        for &sample in samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let pcm = (clamped * i16::MAX as f32) as i16;
            self.writer.write_all(&pcm.to_le_bytes())?;
        }
        self.data_bytes_written += (samples.len() as u32) * (BYTES_PER_SAMPLE as u32);
        Ok(())
    }

    /// Finalize the WAV file by patching the RIFF and data chunk sizes.
    ///
    /// Returns the path to the completed WAV file.
    pub fn finalize(mut self) -> anyhow::Result<PathBuf> {
        self.writer.flush()?;

        // Patch RIFF chunk size (offset 4): total_size - 8
        let riff_size = 36 + self.data_bytes_written;
        self.writer.seek(SeekFrom::Start(4))?;
        self.writer.write_all(&riff_size.to_le_bytes())?;

        // Patch data chunk size (offset 40)
        self.writer.seek(SeekFrom::Start(40))?;
        self.writer
            .write_all(&self.data_bytes_written.to_le_bytes())?;

        self.writer.flush()?;
        Ok(self.path)
    }

    /// Total duration of written audio in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        let total_samples =
            self.data_bytes_written as u64 / (NUM_CHANNELS as u64 * BYTES_PER_SAMPLE as u64);
        (total_samples * 1000) / SAMPLE_RATE as u64
    }

    fn write_header(writer: &mut BufWriter<File>, data_size: u32) -> anyhow::Result<()> {
        let byte_rate = SAMPLE_RATE * NUM_CHANNELS as u32 * BYTES_PER_SAMPLE as u32;
        let block_align = NUM_CHANNELS * BYTES_PER_SAMPLE;

        // RIFF header
        writer.write_all(b"RIFF")?;
        writer.write_all(&(36 + data_size).to_le_bytes())?; // chunk size
        writer.write_all(b"WAVE")?;

        // fmt sub-chunk
        writer.write_all(b"fmt ")?;
        writer.write_all(&16u32.to_le_bytes())?; // sub-chunk size
        writer.write_all(&1u16.to_le_bytes())?; // PCM format
        writer.write_all(&NUM_CHANNELS.to_le_bytes())?;
        writer.write_all(&SAMPLE_RATE.to_le_bytes())?;
        writer.write_all(&byte_rate.to_le_bytes())?;
        writer.write_all(&block_align.to_le_bytes())?;
        writer.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;

        // data sub-chunk
        writer.write_all(b"data")?;
        writer.write_all(&data_size.to_le_bytes())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_wav_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");

        let mut wav = WavWriter::new(&path).unwrap();

        // Write 1 second of silence (16000 samples at 16kHz)
        let silence = vec![0.0f32; 16000];
        wav.write_chunk(&silence).unwrap();

        assert_eq!(wav.duration_ms(), 1000);

        let final_path = wav.finalize().unwrap();
        assert_eq!(final_path, path);

        // Verify file structure with hound
        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        assert_eq!(reader.len(), 16000);
    }

    #[test]
    fn writes_multiple_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multi.wav");

        let mut wav = WavWriter::new(&path).unwrap();

        // Write 3 chunks of 100ms each (1600 samples)
        for _ in 0..3 {
            let chunk = vec![0.5f32; 1600];
            wav.write_chunk(&chunk).unwrap();
        }

        assert_eq!(wav.duration_ms(), 300);
        wav.finalize().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.len(), 4800); // 3 * 1600
    }

    #[test]
    fn clamps_out_of_range_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clamp.wav");

        let mut wav = WavWriter::new(&path).unwrap();
        wav.write_chunk(&[2.0, -2.0, 0.5]).unwrap();
        wav.finalize().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples[0], i16::MAX); // clamped to 1.0
        assert_eq!(samples[1], -i16::MAX); // clamped to -1.0 → -(MAX) since (−1.0 × MAX) = −MAX
    }

    #[test]
    fn creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("test.wav");

        let mut wav = WavWriter::new(&path).unwrap();
        wav.write_chunk(&[0.0; 100]).unwrap();
        wav.finalize().unwrap();

        assert!(path.exists());
    }
}
