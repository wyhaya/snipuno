use rodio::{Decoder, DeviceSinkBuilder, Player, Source, buffer::SamplesBuffer};
use std::{
    io::Cursor,
    sync::LazyLock,
    thread,
    time::{Duration, Instant},
};

// We ensured that the audio was intact during testing.
static CAPTURE_SOUND: LazyLock<SamplesBuffer> =
    LazyLock::new(|| decode_sound(include_bytes!("../../assets/capture.wav")));
static PICK_COLOR_SOUND: LazyLock<SamplesBuffer> =
    LazyLock::new(|| decode_sound(include_bytes!("../../assets/pick-color.wav")));

#[derive(Clone, Copy, Debug)]
pub enum SoundEffect {
    Capture,
    PickColor,
}

impl SoundEffect {
    pub(super) fn play(self) {
        if let Err(error) = thread::Builder::new()
            .name("sound-effect".into())
            .spawn(move || {
                if let Err(error) = play_sound(self) {
                    eprintln!("{error}");
                }
            })
        {
            eprintln!("Unable to start sound effect playback: {error}");
        }
    }

    fn source(self) -> &'static SamplesBuffer {
        match self {
            Self::Capture => &CAPTURE_SOUND,
            Self::PickColor => &PICK_COLOR_SOUND,
        }
    }
}

fn decode_sound(bytes: &'static [u8]) -> SamplesBuffer {
    let decoder = Decoder::try_from(Cursor::new(bytes))
        .expect("Embedded sound effect must be a valid WAV file");
    SamplesBuffer::new(
        decoder.channels(),
        decoder.sample_rate(),
        decoder.collect::<Vec<_>>(),
    )
}

fn play_sound(effect: SoundEffect) -> Result<(), String> {
    let sound = effect.source();
    let mut output = DeviceSinkBuilder::from_default_device()
        .and_then(|builder| builder.open_stream())
        .map_err(|error| format!("Unable to open sound effect output: {error}"))?;
    output.log_on_drop(false);
    let player = Player::connect_new(output.mixer());
    let timeout = sound.total_duration().unwrap_or_default() + Duration::from_secs(2);
    let deadline = Instant::now() + timeout;
    player.append(sound.clone());
    while !player.empty() {
        if Instant::now() >= deadline {
            return Err("Sound effect playback timed out.".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sound_effects_decode_to_short_audible_audio() {
        for effect in [SoundEffect::Capture, SoundEffect::PickColor] {
            let sound = effect.source();
            let duration = sound.total_duration().unwrap();
            assert!(
                duration > Duration::ZERO && duration < Duration::from_secs(1),
                "{effect:?}"
            );
            assert!(sound.clone().all(|sample| sample.is_finite()), "{effect:?}");
            assert!(
                sound.clone().any(|sample| sample.abs() > 0.001),
                "{effect:?}"
            );
        }
    }
}
