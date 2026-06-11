# Third-party licenses

rokugakun is distributed under the MIT License (see `LICENSE`). It builds on open-source Rust crates and bundles a few web assets. **Every dependency is under a permissive license — there is no GPL/LGPL or other copyleft code in the distributed binary.**

This file lists the third-party components and their licenses, to satisfy their attribution requirements. It covers the crates that actually compile into the Windows build (target `x86_64-pc-windows-msvc`).

## Bundled (non-Rust) components

- **hls.js** (`crates/launcher/assets/web/hls.min.js`) — Apache License 2.0, © Dailymotion. https://github.com/video-dev/hls.js — embedded in the web viewer and shipped inside the executable. Its license is reproduced below (Apache-2.0).
- **Default UI fonts** bundled by egui/epaint — Ubuntu Font (UFL-1.0), Hack, and Noto Emoji (OFL-1.1). The launcher also loads a Japanese system font at runtime (not redistributed).
- **FFmpeg is NOT bundled.** rokugakun invokes `ffmpeg`/`ffplay` as external programs if present on your system; they are not included in this distribution, so their licenses (LGPL/GPL depending on your build) do not apply to rokugakun itself. Obtain FFmpeg separately; HEVC/H.264 codec patent licensing for your use is your responsibility.

## License breakdown (Rust crates)

- 151 × `Apache-2.0 OR MIT`
- 25 × `MIT`
- 18 × `Unicode-3.0`
- 12 × `Apache-2.0`
- 7 × `Apache-2.0 OR MIT OR Zlib`
- 5 × `MIT OR Unlicense`
- 2 × `BSL-1.0`
- 2 × `ISC`
- 2 × `Apache-2.0 OR BSD-3-Clause`
- 1 × `0BSD OR Apache-2.0 OR MIT`
- 1 × `(Apache-2.0 OR MIT) AND OFL-1.1 AND LicenseRef-UFL-1.0`
- 1 × `Zlib`
- 1 × `CC0-1.0`
- 1 × `(Apache-2.0 OR MIT) AND Unicode-3.0`
- 1 × `Apache-2.0 OR BSD-2-Clause OR MIT`

All of the above are OSI-approved permissive licenses (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, BSL-1.0, 0BSD, CC0-1.0, Unlicense, Unicode-3.0, OFL-1.1/UFL-1.0 for fonts). `... OR ...` means the crate is multi-licensed and used here under one of the permissive options.

## Dependencies

230 crates:

| Crate | Version | License | Authors | Source |
|---|---|---|---|---|
| ab_glyph | 0.2.32 | Apache-2.0 | Alex Butler <alexheretic@gmail.com> | [link](https://github.com/alexheretic/ab-glyph) |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | Alex Butler <alexheretic@gmail.com> | [link](https://github.com/alexheretic/ab-glyph) |
| accesskit | 0.12.3 | Apache-2.0 OR MIT | The AccessKit contributors | [link](https://github.com/AccessKit/accesskit) |
| accesskit_consumer | 0.16.1 | Apache-2.0 OR MIT | Matt Campbell <mattcampbell@pobox.com> | [link](https://github.com/AccessKit/accesskit) |
| accesskit_windows | 0.15.1 | Apache-2.0 OR MIT | Matt Campbell <mattcampbell@pobox.com> | [link](https://github.com/AccessKit/accesskit) |
| accesskit_winit | 0.16.1 | Apache-2.0 | Matt Campbell <mattcampbell@pobox.com> | [link](https://github.com/AccessKit/accesskit) |
| adler2 | 2.0.1 | 0BSD OR Apache-2.0 OR MIT | Jonas Schievink <jonasschievink@gmail.com>, oyvindln <oyv… | [link](https://github.com/oyvindln/adler2) |
| ahash | 0.8.12 | Apache-2.0 OR MIT | Tom Kaitchuck <Tom.Kaitchuck@gmail.com> | [link](https://github.com/tkaitchuck/ahash) |
| aho-corasick | 1.1.4 | MIT OR Unlicense | Andrew Gallant <jamslam@gmail.com> | [link](https://github.com/BurntSushi/aho-corasick) |
| anyhow | 1.0.102 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/anyhow) |
| arboard | 3.6.1 | Apache-2.0 OR MIT |  | [link](https://github.com/1Password/arboard) |
| arrayvec | 0.7.6 | Apache-2.0 OR MIT | bluss | [link](https://github.com/bluss/arrayvec) |
| ascii | 1.1.0 | Apache-2.0 OR MIT | Thomas Bahn <thomas@thomas-bahn.net>, Torbjørn Birch Molt… | [link](https://github.com/tomprogrammer/rust-ascii) |
| ash | 0.37.3+1.3.251 | Apache-2.0 OR MIT | Maik Klein <maikklein@googlemail.com>, Benjamin Saunders … | [link](https://github.com/MaikKlein/ash) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | Josh Stone <cuviper@gmail.com> | [link](https://github.com/cuviper/autocfg) |
| bit-set | 0.5.3 | Apache-2.0 OR MIT | Alexis Beingessner <a.beingessner@gmail.com> | [link](https://github.com/contain-rs/bit-set) |
| bit-vec | 0.6.3 | Apache-2.0 OR MIT | Alexis Beingessner <a.beingessner@gmail.com> | [link](https://github.com/contain-rs/bit-vec) |
| bitflags | 2.13.0 | Apache-2.0 OR MIT | The Rust Project Developers | [link](https://github.com/bitflags/bitflags) |
| bytemuck | 1.25.0 | Apache-2.0 OR MIT OR Zlib | Lokathor <zefria@gmail.com> | [link](https://github.com/Lokathor/bytemuck) |
| bytemuck_derive | 1.10.2 | Apache-2.0 OR MIT OR Zlib | Lokathor <zefria@gmail.com> | [link](https://github.com/Lokathor/bytemuck) |
| byteorder-lite | 0.1.0 | MIT OR Unlicense |  | [link](https://github.com/image-rs/byteorder-lite) |
| cc | 1.2.63 | Apache-2.0 OR MIT | Alex Crichton <alex@alexcrichton.com> | [link](https://github.com/rust-lang/cc-rs) |
| cfg-if | 1.0.4 | Apache-2.0 OR MIT | Alex Crichton <alex@alexcrichton.com> | [link](https://github.com/rust-lang/cfg-if) |
| cfg_aliases | 0.1.1 | MIT | Zicklag <zicklag@katharostech.com> | [link](https://github.com/katharostech/cfg_aliases) |
| chunked_transfer | 1.5.0 | Apache-2.0 OR MIT | Corey Farwell <coreyf@rwell.org> | [link](https://github.com/frewsxcv/rust-chunked-transfer) |
| clipboard-win | 5.4.1 | BSL-1.0 | Douman <douman@gmx.se> | [link](https://github.com/DoumanAsh/clipboard-win) |
| codespan-reporting | 0.11.1 | Apache-2.0 | Brendan Zabarauskas <bjzaba@yahoo.com.au> | [link](https://github.com/brendanzab/codespan) |
| com | 0.6.0 | MIT | Microsoft Corp. | [link](https://github.com/microsoft/com-rs) |
| com_macros | 0.6.0 | MIT | Microsoft Corp. | [link](https://github.com/microsoft/com-rs) |
| com_macros_support | 0.6.0 | MIT | Microsoft Corp. | [link](https://github.com/microsoft/com-rs) |
| crc32fast | 1.5.0 | Apache-2.0 OR MIT | Sam Rijs <srijs@airpost.net>, Alex Crichton <alex@alexcri… | [link](https://github.com/srijs/rust-crc32fast) |
| crossbeam-channel | 0.5.15 | Apache-2.0 OR MIT |  | [link](https://github.com/crossbeam-rs/crossbeam) |
| crossbeam-utils | 0.8.21 | Apache-2.0 OR MIT |  | [link](https://github.com/crossbeam-rs/crossbeam) |
| cursor-icon | 1.2.0 | Apache-2.0 OR MIT OR Zlib | Kirill Chibisov <contact@kchibisov.com> | [link](https://github.com/rust-windowing/cursor-icon) |
| deranged | 0.5.8 | Apache-2.0 OR MIT | Jacob Pratt <jacob@jhpratt.dev> | [link](https://github.com/jhpratt/deranged) |
| displaydoc | 0.2.6 | Apache-2.0 OR MIT | Jane Lusby <jlusby@yaah.dev> | [link](https://github.com/yaahc/displaydoc) |
| document-features | 0.2.12 | Apache-2.0 OR MIT | Slint Developers <info@slint.dev> | [link](https://github.com/slint-ui/document-features) |
| ecolor | 0.28.1 | Apache-2.0 OR MIT | Emil Ernerfeldt <emil.ernerfeldt@gmail.com>, Andreas Reic… | [link](https://github.com/emilk/egui) |
| eframe | 0.28.1 | Apache-2.0 OR MIT | Emil Ernerfeldt <emil.ernerfeldt@gmail.com> | [link](https://github.com/emilk/egui/tree/master/crates/eframe) |
| egui | 0.28.1 | Apache-2.0 OR MIT | Emil Ernerfeldt <emil.ernerfeldt@gmail.com> | [link](https://github.com/emilk/egui) |
| egui-wgpu | 0.28.1 | Apache-2.0 OR MIT | Nils Hasenbanck <nils@hasenbanck.de>, embotech <opensourc… | [link](https://github.com/emilk/egui/tree/master/crates/egui-wgpu) |
| egui-winit | 0.28.1 | Apache-2.0 OR MIT | Emil Ernerfeldt <emil.ernerfeldt@gmail.com> | [link](https://github.com/emilk/egui/tree/master/crates/egui-winit) |
| egui_glow | 0.28.1 | Apache-2.0 OR MIT | Emil Ernerfeldt <emil.ernerfeldt@gmail.com> | [link](https://github.com/emilk/egui/tree/master/crates/egui_glow) |
| emath | 0.28.1 | Apache-2.0 OR MIT | Emil Ernerfeldt <emil.ernerfeldt@gmail.com> | [link](https://github.com/emilk/egui/tree/master/crates/emath) |
| epaint | 0.28.1 | (Apache-2.0 OR MIT) AND OFL-1.1 AND LicenseRef-UFL-1.0 | Emil Ernerfeldt <emil.ernerfeldt@gmail.com> | [link](https://github.com/emilk/egui/tree/master/crates/epaint) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |  | [link](https://github.com/indexmap-rs/equivalent) |
| error-code | 3.3.2 | BSL-1.0 | Douman <douman@gmx.se> | [link](https://github.com/DoumanAsh/error-code) |
| fallible-iterator | 0.3.0 | Apache-2.0 OR MIT | Steven Fackler <sfackler@gmail.com> | [link](https://github.com/sfackler/rust-fallible-iterator) |
| fallible-streaming-iterator | 0.1.9 | Apache-2.0 OR MIT | Steven Fackler <sfackler@gmail.com> | [link](https://github.com/sfackler/fallible-streaming-iterator) |
| fastrand | 2.4.1 | Apache-2.0 OR MIT | Stjepan Glavina <stjepang@gmail.com> | [link](https://github.com/smol-rs/fastrand) |
| fdeflate | 0.3.7 | Apache-2.0 OR MIT | The image-rs Developers | [link](https://github.com/image-rs/fdeflate) |
| find-msvc-tools | 0.1.9 | Apache-2.0 OR MIT |  | [link](https://github.com/rust-lang/cc-rs) |
| flate2 | 1.1.9 | Apache-2.0 OR MIT | Alex Crichton <alex@alexcrichton.com>, Josh Triplett <jos… | [link](https://github.com/rust-lang/flate2-rs) |
| foldhash | 0.1.5 | Zlib | Orson Peters <orsonpeters@gmail.com> | [link](https://github.com/orlp/foldhash) |
| form_urlencoded | 1.2.2 | Apache-2.0 OR MIT | The rust-url developers | [link](https://github.com/servo/rust-url) |
| getrandom | 0.3.4 | Apache-2.0 OR MIT | The Rand Project Developers | [link](https://github.com/rust-random/getrandom) |
| getrandom | 0.4.2 | Apache-2.0 OR MIT | The Rand Project Developers | [link](https://github.com/rust-random/getrandom) |
| gl_generator | 0.14.0 | Apache-2.0 | Brendan Zabarauskas <bjzaba@yahoo.com.au>, Corey Richards… | [link](https://github.com/brendanzab/gl-rs/) |
| glow | 0.13.1 | Apache-2.0 OR MIT OR Zlib | Joshua Groves <josh@joshgroves.com>, Dzmitry Malyshau <kv… | [link](https://github.com/grovesNL/glow) |
| glutin | 0.31.3 | Apache-2.0 | Kirill Chibisov <contact@kchibisov.com> | [link](https://github.com/rust-windowing/glutin) |
| glutin-winit | 0.4.2 | MIT | Kirill Chibisov <contact@kchibisov.com> | [link](https://github.com/rust-windowing/glutin) |
| glutin_egl_sys | 0.6.0 | Apache-2.0 | Kirill Chibisov <contact@kchibisov.com> | [link](https://github.com/rust-windowing/glutin) |
| glutin_wgl_sys | 0.5.0 | Apache-2.0 | Kirill Chibisov <contact@kchibisov.com> | [link](https://github.com/rust-windowing/glutin) |
| gpu-alloc | 0.6.0 | Apache-2.0 OR MIT | Zakarum <zakarumych@ya.ru> | [link](https://github.com/zakarumych/gpu-alloc) |
| gpu-alloc-types | 0.3.0 | Apache-2.0 OR MIT | Zakarum <zakarumych@ya.ru> | [link](https://github.com/zakarumych/gpu-alloc) |
| gpu-allocator | 0.25.0 | Apache-2.0 OR MIT | Traverse Research <opensource@traverseresearch.nl> | [link](https://github.com/Traverse-Research/gpu-allocator) |
| gpu-descriptor | 0.3.2 | Apache-2.0 OR MIT | Zakarum <zakarumych@ya.ru> | [link](https://github.com/zakarumych/gpu-descriptor) |
| gpu-descriptor-types | 0.2.0 | Apache-2.0 OR MIT | Zakarum <zakarumych@ya.ru> | [link](https://github.com/zakarumych/gpu-descriptor) |
| hashbrown | 0.14.5 | Apache-2.0 OR MIT | Amanieu d'Antras <amanieu@gmail.com> | [link](https://github.com/rust-lang/hashbrown) |
| hashbrown | 0.15.5 | Apache-2.0 OR MIT | Amanieu d'Antras <amanieu@gmail.com> | [link](https://github.com/rust-lang/hashbrown) |
| hashbrown | 0.17.1 | Apache-2.0 OR MIT |  | [link](https://github.com/rust-lang/hashbrown) |
| hashlink | 0.9.1 | Apache-2.0 OR MIT | kyren <kerriganw@gmail.com> | [link](https://github.com/kyren/hashlink) |
| hassle-rs | 0.11.0 | MIT | Traverse-Research <support@traverseresearch.nl> | [link](https://github.com/Traverse-Research/hassle-rs) |
| hexf-parse | 0.2.1 | CC0-1.0 | Kang Seonghoon <public+rust@mearie.org> | [link](https://github.com/lifthrasiir/hexf) |
| httpdate | 1.0.3 | Apache-2.0 OR MIT | Pyfisch <pyfisch@posteo.org> | [link](https://github.com/pyfisch/httpdate) |
| icu_collections | 2.2.0 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| icu_locale_core | 2.2.0 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| icu_normalizer | 2.2.0 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| icu_normalizer_data | 2.2.0 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| icu_properties | 2.2.0 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| icu_properties_data | 2.2.0 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| icu_provider | 2.2.0 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| idna | 1.1.0 | Apache-2.0 OR MIT | The rust-url developers | [link](https://github.com/servo/rust-url/) |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | The rust-url developers | [link](https://github.com/hsivonen/idna_adapter) |
| image | 0.25.10 | Apache-2.0 OR MIT | The image-rs Developers | [link](https://github.com/image-rs/image) |
| indexmap | 2.14.0 | Apache-2.0 OR MIT |  | [link](https://github.com/indexmap-rs/indexmap) |
| itoa | 1.0.18 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/itoa) |
| jobserver | 0.1.34 | Apache-2.0 OR MIT | Alex Crichton <alex@alexcrichton.com> | [link](https://github.com/rust-lang/jobserver-rs) |
| khronos-egl | 6.0.0 | Apache-2.0 OR MIT | Timothée Haudebourg <author@haudebourg.net>, Sean Kerr <s… | [link](https://github.com/timothee-haudebourg/khronos-egl) |
| khronos_api | 3.1.0 | Apache-2.0 | Brendan Zabarauskas <bjzaba@yahoo.com.au>, Corey Richards… | [link](https://github.com/brendanzab/gl-rs/) |
| lazy_static | 1.5.0 | Apache-2.0 OR MIT | Marvin Löbel <loebel.marvin@gmail.com> | [link](https://github.com/rust-lang-nursery/lazy-static.rs) |
| libc | 0.2.186 | Apache-2.0 OR MIT | The Rust Project Developers | [link](https://github.com/rust-lang/libc) |
| libloading | 0.7.4 | ISC | Simonas Kazlauskas <libloading@kazlauskas.me> | [link](https://github.com/nagisa/rust_libloading/) |
| libloading | 0.8.9 | ISC | Simonas Kazlauskas <libloading@kazlauskas.me> | [link](https://github.com/nagisa/rust_libloading/) |
| libsqlite3-sys | 0.30.1 | MIT | The rusqlite developers | [link](https://github.com/rusqlite/rusqlite) |
| litemap | 0.8.2 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| litrs | 1.0.0 | Apache-2.0 OR MIT | Lukas Kalbertodt <lukas.kalbertodt@gmail.com> | [link](https://github.com/LukasKalbertodt/litrs) |
| lock_api | 0.4.14 | Apache-2.0 OR MIT | Amanieu d'Antras <amanieu@gmail.com> | [link](https://github.com/Amanieu/parking_lot) |
| log | 0.4.32 | Apache-2.0 OR MIT | The Rust Project Developers | [link](https://github.com/rust-lang/log) |
| matchers | 0.2.0 | MIT | Eliza Weisman <eliza@buoyant.io> | [link](https://github.com/hawkw/matchers) |
| memchr | 2.8.1 | MIT OR Unlicense | Andrew Gallant <jamslam@gmail.com>, bluss | [link](https://github.com/BurntSushi/memchr) |
| memoffset | 0.9.1 | MIT | Gilad Naaman <gilad.naaman@gmail.com> | [link](https://github.com/Gilnaa/memoffset) |
| miniz_oxide | 0.8.9 | Apache-2.0 OR MIT OR Zlib | Frommi <daniil.liferenko@gmail.com>, oyvindln <oyvindln@u… | [link](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) |
| moxcms | 0.8.1 | Apache-2.0 OR BSD-3-Clause | Radzivon Bartoshyk | [link](https://github.com/awxkee/moxcms.git) |
| naga | 0.20.0 | Apache-2.0 OR MIT | gfx-rs developers | [link](https://github.com/gfx-rs/wgpu/tree/trunk/naga) |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | Parity Technologies <admin@parity.io> | [link](https://github.com/paritytech/nohash-hasher) |
| nu-ansi-term | 0.50.3 | MIT | ogham@bsago.me, Ryan Scheel (Havvy) <ryan.havvy@gmail.com… | [link](https://github.com/nushell/nu-ansi-term) |
| num-conv | 0.2.2 | Apache-2.0 OR MIT | Jacob Pratt <jacob@jhpratt.dev> | [link](https://github.com/jhpratt/num-conv) |
| num-traits | 0.2.19 | Apache-2.0 OR MIT | The Rust Project Developers | [link](https://github.com/rust-num/num-traits) |
| once_cell | 1.21.4 | Apache-2.0 OR MIT | Aleksey Kladov <aleksey.kladov@gmail.com> | [link](https://github.com/matklad/once_cell) |
| owned_ttf_parser | 0.25.1 | Apache-2.0 | Alex Butler <alexheretic@gmail.com> | [link](https://github.com/alexheretic/owned-ttf-parser) |
| parking_lot | 0.12.5 | Apache-2.0 OR MIT | Amanieu d'Antras <amanieu@gmail.com> | [link](https://github.com/Amanieu/parking_lot) |
| parking_lot_core | 0.9.12 | Apache-2.0 OR MIT | Amanieu d'Antras <amanieu@gmail.com> | [link](https://github.com/Amanieu/parking_lot) |
| paste | 1.0.15 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/paste) |
| percent-encoding | 2.3.2 | Apache-2.0 OR MIT | The rust-url developers | [link](https://github.com/servo/rust-url/) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |  | [link](https://github.com/taiki-e/pin-project-lite) |
| pkg-config | 0.3.33 | Apache-2.0 OR MIT | Alex Crichton <alex@alexcrichton.com> | [link](https://github.com/rust-lang/pkg-config-rs) |
| png | 0.18.1 | Apache-2.0 OR MIT | The image-rs Developers | [link](https://github.com/image-rs/image-png) |
| potential_utf | 0.1.5 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| powerfmt | 0.2.0 | Apache-2.0 OR MIT | Jacob Pratt <jacob@jhpratt.dev> | [link](https://github.com/jhpratt/powerfmt) |
| presser | 0.3.1 | Apache-2.0 OR MIT | Embark <opensource@embark-studios.com>, Gray Olson <gray@… | [link](https://github.com/EmbarkStudios/presser) |
| proc-macro2 | 1.0.106 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com>, Alex Crichton <alex@ale… | [link](https://github.com/dtolnay/proc-macro2) |
| profiling | 1.0.18 | Apache-2.0 OR MIT | Philip Degarmo <aclysma@gmail.com> | [link](https://github.com/aclysma/profiling) |
| pxfm | 0.1.29 | Apache-2.0 OR BSD-3-Clause | Radzivon Bartoshyk | [link](https://github.com/awxkee/pxfm) |
| quote | 1.0.45 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/quote) |
| raw-window-handle | 0.5.2 | Apache-2.0 OR MIT OR Zlib | Osspial <osspial@gmail.com> | [link](https://github.com/rust-windowing/raw-window-handle) |
| raw-window-handle | 0.6.2 | Apache-2.0 OR MIT OR Zlib | Osspial <osspial@gmail.com> | [link](https://github.com/rust-windowing/raw-window-handle) |
| regex | 1.12.3 | Apache-2.0 OR MIT | The Rust Project Developers, Andrew Gallant <jamslam@gmai… | [link](https://github.com/rust-lang/regex) |
| regex-automata | 0.4.14 | Apache-2.0 OR MIT | The Rust Project Developers, Andrew Gallant <jamslam@gmai… | [link](https://github.com/rust-lang/regex) |
| regex-syntax | 0.8.10 | Apache-2.0 OR MIT | The Rust Project Developers, Andrew Gallant <jamslam@gmai… | [link](https://github.com/rust-lang/regex) |
| renderdoc-sys | 1.1.0 | Apache-2.0 OR MIT | Eyal Kalderon <ebkalderon@gmail.com> | [link](https://github.com/ebkalderon/renderdoc-rs) |
| rfd | 0.14.1 | MIT | Poly <marynczak.bartlomiej@gmail.com> | [link](https://github.com/PolyMeilex/rfd) |
| rusqlite | 0.32.1 | MIT | The rusqlite developers | [link](https://github.com/rusqlite/rusqlite) |
| rustc-hash | 1.1.0 | Apache-2.0 OR MIT | The Rust Project Developers | [link](https://github.com/rust-lang-nursery/rustc-hash) |
| rustc-hash | 2.1.2 | Apache-2.0 OR MIT | The Rust Project Developers | [link](https://github.com/rust-lang/rustc-hash) |
| scopeguard | 1.2.0 | Apache-2.0 OR MIT | bluss | [link](https://github.com/bluss/scopeguard) |
| serde | 1.0.228 | Apache-2.0 OR MIT | Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay… | [link](https://github.com/serde-rs/serde) |
| serde_core | 1.0.228 | Apache-2.0 OR MIT | Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay… | [link](https://github.com/serde-rs/serde) |
| serde_derive | 1.0.228 | Apache-2.0 OR MIT | Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay… | [link](https://github.com/serde-rs/serde) |
| serde_json | 1.0.150 | Apache-2.0 OR MIT | Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay… | [link](https://github.com/serde-rs/json) |
| serde_spanned | 1.1.1 | Apache-2.0 OR MIT |  | [link](https://github.com/toml-rs/toml) |
| sharded-slab | 0.1.7 | MIT | Eliza Weisman <eliza@buoyant.io> | [link](https://github.com/hawkw/sharded-slab) |
| shlex | 2.0.1 | Apache-2.0 OR MIT | comex <comexk@gmail.com>, Fenhl <fenhl@fenhl.net>, Adrian… | [link](https://github.com/comex/rust-shlex) |
| simd-adler32 | 0.3.9 | MIT | Marvin Countryman <me@maar.vin> | [link](https://github.com/mcountryman/simd-adler32) |
| smallvec | 1.15.1 | Apache-2.0 OR MIT | The Servo Project Developers | [link](https://github.com/servo/rust-smallvec) |
| smol_str | 0.2.2 | Apache-2.0 OR MIT | Aleksey Kladov <aleksey.kladov@gmail.com> | [link](https://github.com/rust-analyzer/smol_str) |
| spirv | 0.3.0+sdk-1.3.268.0 | Apache-2.0 | Lei Zhang <antiagainst@gmail.com> | [link](https://github.com/gfx-rs/rspirv) |
| stable_deref_trait | 1.2.1 | Apache-2.0 OR MIT | Robert Grosse <n210241048576@gmail.com> | [link](https://github.com/storyyeller/stable_deref_trait) |
| static_assertions | 1.1.0 | Apache-2.0 OR MIT | Nikolai Vazquez | [link](https://github.com/nvzqz/static-assertions-rs) |
| symlink | 0.1.0 | Apache-2.0 OR MIT | Chris Morgan <me@chrismorgan.info> | [link](https://gitlab.com/chris-morgan/symlink) |
| syn | 1.0.109 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/syn) |
| syn | 2.0.117 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/syn) |
| synstructure | 0.13.2 | MIT | Nika Layzell <nika@thelayzells.com> | [link](https://github.com/mystor/synstructure) |
| tempfile | 3.27.0 | Apache-2.0 OR MIT | Steven Allen <steven@stebalien.com>, The Rust Project Dev… | [link](https://github.com/Stebalien/tempfile) |
| termcolor | 1.4.1 | MIT OR Unlicense | Andrew Gallant <jamslam@gmail.com> | [link](https://github.com/BurntSushi/termcolor) |
| thiserror | 1.0.69 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/thiserror) |
| thiserror | 2.0.18 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/thiserror) |
| thiserror-impl | 1.0.69 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/thiserror) |
| thiserror-impl | 2.0.18 | Apache-2.0 OR MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/thiserror) |
| thread_local | 1.1.9 | Apache-2.0 OR MIT | Amanieu d'Antras <amanieu@gmail.com> | [link](https://github.com/Amanieu/thread_local-rs) |
| time | 0.3.47 | Apache-2.0 OR MIT | Jacob Pratt <open-source@jhpratt.dev>, Time contributors | [link](https://github.com/time-rs/time) |
| time-core | 0.1.8 | Apache-2.0 OR MIT | Jacob Pratt <open-source@jhpratt.dev>, Time contributors | [link](https://github.com/time-rs/time) |
| time-macros | 0.2.27 | Apache-2.0 OR MIT | Jacob Pratt <open-source@jhpratt.dev>, Time contributors | [link](https://github.com/time-rs/time) |
| tiny_http | 0.12.0 | Apache-2.0 OR MIT | pierre.krieger1708@gmail.com, Corey Farwell <coreyf@rwell… | [link](https://github.com/tiny-http/tiny-http) |
| tinystr | 0.8.3 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| toml | 1.1.2+spec-1.1.0 | Apache-2.0 OR MIT |  | [link](https://github.com/toml-rs/toml) |
| toml_datetime | 1.1.1+spec-1.1.0 | Apache-2.0 OR MIT |  | [link](https://github.com/toml-rs/toml) |
| toml_parser | 1.1.2+spec-1.1.0 | Apache-2.0 OR MIT |  | [link](https://github.com/toml-rs/toml) |
| toml_writer | 1.1.1+spec-1.1.0 | Apache-2.0 OR MIT |  | [link](https://github.com/toml-rs/toml) |
| tracing | 0.1.44 | MIT | Eliza Weisman <eliza@buoyant.io>, Tokio Contributors <tea… | [link](https://github.com/tokio-rs/tracing) |
| tracing-appender | 0.2.5 | MIT | Zeki Sherif <zekshi@amazon.com>, Tokio Contributors <team… | [link](https://github.com/tokio-rs/tracing) |
| tracing-attributes | 0.1.31 | MIT | Tokio Contributors <team@tokio.rs>, Eliza Weisman <eliza@… | [link](https://github.com/tokio-rs/tracing) |
| tracing-core | 0.1.36 | MIT | Tokio Contributors <team@tokio.rs> | [link](https://github.com/tokio-rs/tracing) |
| tracing-log | 0.2.0 | MIT | Tokio Contributors <team@tokio.rs> | [link](https://github.com/tokio-rs/tracing) |
| tracing-subscriber | 0.3.23 | MIT | Eliza Weisman <eliza@buoyant.io>, David Barsky <me@davidb… | [link](https://github.com/tokio-rs/tracing) |
| ttf-parser | 0.25.1 | Apache-2.0 OR MIT | Caleb Maclennan <caleb@alerque.com>, Laurenz Stampfl <lau… | [link](https://github.com/harfbuzz/ttf-parser) |
| type-map | 0.5.1 | Apache-2.0 OR MIT | Jacob Brown <kardeiz@gmail.com> | [link](https://github.com/kardeiz/type-map) |
| unicode-ident | 1.0.24 | (Apache-2.0 OR MIT) AND Unicode-3.0 | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/unicode-ident) |
| unicode-segmentation | 1.13.3 | Apache-2.0 OR MIT | kwantam <kwantam@gmail.com>, Manish Goregaokar <manishsma… | [link](https://github.com/unicode-rs/unicode-segmentation) |
| unicode-width | 0.1.14 | Apache-2.0 OR MIT | kwantam <kwantam@gmail.com>, Manish Goregaokar <manishsma… | [link](https://github.com/unicode-rs/unicode-width) |
| unicode-xid | 0.2.6 | Apache-2.0 OR MIT | erick.tryzelaar <erick.tryzelaar@gmail.com>, kwantam <kwa… | [link](https://github.com/unicode-rs/unicode-xid) |
| url | 2.5.8 | Apache-2.0 OR MIT | The rust-url developers | [link](https://github.com/servo/rust-url) |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | Henri Sivonen <hsivonen@hsivonen.fi> | [link](https://github.com/hsivonen/utf8_iter) |
| uuid | 1.23.2 | Apache-2.0 OR MIT | Ashley Mannix<ashleymannix@live.com.au>, Dylan DPC<dylan.… | [link](https://github.com/uuid-rs/uuid) |
| vcpkg | 0.2.15 | Apache-2.0 OR MIT | Jim McGrath <jimmc2@gmail.com> | [link](https://github.com/mcgoo/vcpkg-rs) |
| version_check | 0.9.5 | Apache-2.0 OR MIT | Sergio Benitez <sb@sergio.bz> | [link](https://github.com/SergioBenitez/version_check) |
| web-time | 0.2.4 | Apache-2.0 OR MIT |  | [link](https://github.com/daxpedda/web-time) |
| webbrowser | 1.2.1 | Apache-2.0 OR MIT | Amod Malviya @amodm | [link](https://github.com/amodm/webbrowser-rs) |
| wgpu | 0.20.1 | Apache-2.0 OR MIT | gfx-rs developers | [link](https://github.com/gfx-rs/wgpu) |
| wgpu-core | 0.21.1 | Apache-2.0 OR MIT | gfx-rs developers | [link](https://github.com/gfx-rs/wgpu) |
| wgpu-hal | 0.21.1 | Apache-2.0 OR MIT | gfx-rs developers | [link](https://github.com/gfx-rs/wgpu) |
| wgpu-types | 0.20.0 | Apache-2.0 OR MIT | gfx-rs developers | [link](https://github.com/gfx-rs/wgpu) |
| widestring | 1.2.1 | Apache-2.0 OR MIT |  | [link](https://github.com/VoidStarKat/widestring-rs) |
| winapi | 0.3.9 | Apache-2.0 OR MIT | Peter Atashian <retep998@gmail.com> | [link](https://github.com/retep998/winapi-rs) |
| winapi-util | 0.1.11 | MIT OR Unlicense | Andrew Gallant <jamslam@gmail.com> | [link](https://github.com/BurntSushi/winapi-util) |
| windows | 0.48.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows | 0.52.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows | 0.58.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-core | 0.52.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-core | 0.58.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-implement | 0.48.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-implement | 0.58.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-interface | 0.48.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-interface | 0.58.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-link | 0.2.1 | Apache-2.0 OR MIT |  | [link](https://github.com/microsoft/windows-rs) |
| windows-result | 0.2.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-strings | 0.1.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-sys | 0.48.0 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-sys | 0.60.2 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-sys | 0.61.2 | Apache-2.0 OR MIT |  | [link](https://github.com/microsoft/windows-rs) |
| windows-targets | 0.48.5 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-targets | 0.52.6 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows-targets | 0.53.5 | Apache-2.0 OR MIT |  | [link](https://github.com/microsoft/windows-rs) |
| windows_x86_64_msvc | 0.48.5 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows_x86_64_msvc | 0.52.6 | Apache-2.0 OR MIT | Microsoft | [link](https://github.com/microsoft/windows-rs) |
| windows_x86_64_msvc | 0.53.1 | Apache-2.0 OR MIT |  | [link](https://github.com/microsoft/windows-rs) |
| winit | 0.29.15 | Apache-2.0 | The winit contributors, Pierre Krieger <pierre.krieger170… | [link](https://github.com/rust-windowing/winit) |
| winnow | 1.0.3 | MIT |  | [link](https://github.com/winnow-rs/winnow) |
| winresource | 0.1.31 | MIT | Max Resch <resch.max@gmail.com> | [link](https://github.com/BenjaminRi/winresource) |
| writeable | 0.6.3 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| xml-rs | 0.8.28 | MIT | Vladimir Matveev <vmatveev@citrine.cc> | [link](https://github.com/kornelski/xml-rs) |
| yoke | 0.8.3 | Unicode-3.0 | Manish Goregaokar <manishsmail@gmail.com> | [link](https://github.com/unicode-org/icu4x) |
| yoke-derive | 0.8.2 | Unicode-3.0 | Manish Goregaokar <manishsmail@gmail.com> | [link](https://github.com/unicode-org/icu4x) |
| zerocopy | 0.8.50 | Apache-2.0 OR BSD-2-Clause OR MIT | Joshua Liebow-Feeser <joshlf@google.com>, Jack Wrenn <jsw… | [link](https://github.com/google/zerocopy) |
| zerofrom | 0.1.8 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| zerofrom-derive | 0.1.7 | Unicode-3.0 | Manish Goregaokar <manishsmail@gmail.com> | [link](https://github.com/unicode-org/icu4x) |
| zerotrie | 0.2.4 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| zerovec | 0.11.6 | Unicode-3.0 | The ICU4X Project Developers | [link](https://github.com/unicode-org/icu4x) |
| zerovec-derive | 0.11.3 | Unicode-3.0 | Manish Goregaokar <manishsmail@gmail.com> | [link](https://github.com/unicode-org/icu4x) |
| zmij | 1.0.21 | MIT | David Tolnay <dtolnay@gmail.com> | [link](https://github.com/dtolnay/zmij) |

## Full license texts of the major licenses

For each SPDX identifier above, the canonical full text is published at `https://spdx.org/licenses/<ID>.html` (e.g. https://spdx.org/licenses/ISC.html). The two most common licenses in this project — MIT and Apache-2.0 — are reproduced in full below. Per-crate copyright holders are listed in the Authors column above and in each crate's repository.

### MIT License

```
MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Apache License 2.0

Applies to the Apache-2.0 / `Apache-2.0 OR ...` crates above and to the bundled hls.js.

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```
