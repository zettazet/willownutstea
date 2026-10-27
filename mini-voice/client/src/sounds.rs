//! Original soft sine chimes, embedded PCM; OS playback, no application worker.
use windows_sys::Win32::Media::Audio::*;
#[derive(Clone, Copy)]
pub enum Cue {
    Call,
    Mute,
    Unmute,
}
// WAVE headers are read as words by WinMM, so make alignment explicit.
#[repr(align(4))]
struct Wave<const N: usize>([u8; N]);
static CALL: Wave<{ include_bytes!("../assets/call.wav").len() }> =
    Wave(*include_bytes!("../assets/call.wav"));
static MUTE: Wave<{ include_bytes!("../assets/mute.wav").len() }> =
    Wave(*include_bytes!("../assets/mute.wav"));
static UNMUTE: Wave<{ include_bytes!("../assets/unmute.wav").len() }> =
    Wave(*include_bytes!("../assets/unmute.wav"));
pub fn play(cue: Cue) {
    let bytes: &[u8] = match cue {
        Cue::Call => &CALL.0,
        Cue::Mute => &MUTE.0,
        Cue::Unmute => &UNMUTE.0,
    };
    unsafe {
        PlaySoundW(
            bytes.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}
pub fn stop() {
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
}
