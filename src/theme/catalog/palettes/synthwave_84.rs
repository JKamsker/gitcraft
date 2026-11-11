//! Published synthwave_84 palette values.
use super::super::*;

pub(in super::super) const SYNTHWAVE_84: Builtin = Builtin {
    id: "synthwave-84",
    name: "SynthWave '84",
    credit: "SynthWave '84 · Robb Owen",
    palette: pal(
        true,
        [0x171520, 0x262335, 0x241b2f, 0x2a2139, 0x495495],
        [0xffffff, 0xe0dbe8, 0x848bbd],
        0xff7edb,
        [
            0xfe4450, 0x72f1b8, 0xfede5d, 0x2ee2fa, 0xff7edb, 0x36f9f6, 0xf97e72,
        ],
    )
    .with_selection(hex(0x463465)),
};
