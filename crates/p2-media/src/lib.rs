//! P2 medya katmanı: sesli arama motoru (WebRTC örgü + ses aygıtları + yankı giderme),
//! zil ve işaret sesleri. Arayüzden bağımsızdır; istemci (p2-client) komut gönderir,
//! olay alır, sinyalleşmeyi socket.io üzerinden kendisi taşır.

pub mod audio;
pub mod camera;
pub mod engine;
pub mod loopback;
pub mod synth;
pub mod video;

pub use audio::{DeviceInfo, input_devices, output_devices};
pub use camera::{CameraInfo, CameraSource, cameras};
pub use engine::{Command, Engine, Event, IceConfig, MediaConfig, bitrate_for, quality_preset};
pub use synth::{Cue, Ring};
pub use video::{Frames, ScreenInfo, ScreenSource, VideoFrameData, screen_sources};
