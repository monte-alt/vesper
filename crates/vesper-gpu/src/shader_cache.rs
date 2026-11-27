//! Translated shaders kept on disk (decision 0003): under the configured
//! cache folder, keyed by the container's content, by the translator's
//! version ([`vesper_shader::TRANSLATOR_VERSION`]) and by its settings.
//! A rebuild that leaves the version keeps every translation: the store
//! grows by the shaders a change made translate, not by a whole new set
//! each build (16 GB of sets of earlier builds had piled up when the key
//! was the translator's source). Entries go through `vesper-pipeline`'s
//! content-addressed cache, which writes them atomically; an entry that
//! does not read back is translated again.
//!
//! Each entry also names the build of the translator that made it (the
//! BLAKE3 of its sources, `build.rs`). One of an earlier build is served
//! as it is, and counted; `vesper shaders` ([`precompile`]) translates it
//! again, keeps the new one and says how many came out different (then
//! the version should have been bumped). `VESPER_SHADER_FRESH=1` does the
//! same in a run; `VESPER_SHADER_KEY=source` keys by the translator's
//! source as before (a separate set for each build).
//!
//! The entry is our own encoding of [`HostShader`]: a field added to it
//! must be added to `encode` and `decode` too, and [`FORMAT`] bumped.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// The encoding of entries ([`encode`]), part of the key.
const FORMAT: u32 = 3;

use vesper_pipeline::{Cache, Converter};
use vesper_shader::emit::{HostShader, StaticSampler, UserData};
use vesper_shader::resource::RootIndexedView;
use vesper_shader::{HwStage, RootType};

static CACHE: OnceLock<Cache> = OnceLock::new();
static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
/// Hits on entries an earlier build of the translator made (served as they
/// are, or when refreshing translated again); of those translated again,
/// how many came out different.
static STALE: AtomicU64 = AtomicU64::new(0);
static REFRESHED: AtomicU64 = AtomicU64::new(0);
static CHANGED: AtomicU64 = AtomicU64::new(0);

/// The build of the translator (the BLAKE3 of its sources, `build.rs`).
const SOURCE: &str = env!("VESPER_TRANSLATOR");

/// How cached translations are keyed (`VESPER_SHADER_KEY`).
fn keyed_by_source() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("VESPER_SHADER_KEY").is_ok_and(|v| v == "source"))
}

/// Whether a run translates entries of an earlier build again
/// (`VESPER_SHADER_FRESH=1`; [`precompile`] always does).
fn refresh_in_runs() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VESPER_SHADER_FRESH").is_some_and(|v| v == "1"))
}

/// Keeps translated shaders in `dir` from now on.
pub fn use_shader_cache(dir: PathBuf) {
    match Cache::open(dir) {
        Ok(cache) => {
            let _ = CACHE.set(cache);
        }
        Err(e) => tracing::warn!(error = %e, "no shader cache"),
    }
}

/// Shaders found in the cache and translated since the start.
pub(crate) fn counts() -> (u64, u64) {
    (HITS.load(Ordering::Relaxed), MISSES.load(Ordering::Relaxed))
}

/// Since the start: entries of an earlier build of the translator met,
/// those of them translated again, and those that came out different.
pub fn stale_counts() -> (u64, u64, u64) {
    (STALE.load(Ordering::Relaxed), REFRESHED.load(Ordering::Relaxed), CHANGED.load(Ordering::Relaxed))
}

/// What identifies a translation beside its container: the translator's
/// version (or with `VESPER_SHADER_KEY=source` its sources), the form of
/// entries, the settings it runs with, and what it was told of the host.
/// Not our executable: a rebuild of anything else used to translate every
/// shader again.
fn build() -> [u8; 32] {
    // The view heap's null slot differs by host GPU: a translation cached
    // without the clamp must not be served (`host_settings` below).
    crate::descriptor::host::probe_view_limit();
    static FIXED: OnceLock<Vec<u8>> = OnceLock::new();
    let fixed = FIXED.get_or_init(|| {
        let limit = vesper_shader::emit::loop_limit();
        if limit != vesper_shader::emit::DEFAULT_LOOP_LIMIT {
            tracing::warn!(
                limit,
                "VESPER_SHADER_LOOP_LIMIT: translated loops stop sooner"
            );
        }
        let mut bytes = if keyed_by_source() {
            SOURCE.as_bytes().to_vec()
        } else {
            format!("version {}", vesper_shader::TRANSLATOR_VERSION).into_bytes()
        };
        bytes.extend_from_slice(&FORMAT.to_le_bytes());
        bytes.extend_from_slice(&limit.to_le_bytes());
        // Which group barriers wait is part of the translation too.
        bytes.push(vesper_shader::emit::group_sync());
        // So are the switches that pick the form of a wave64 program:
        // without them here a run with `VESPER_WAVE_WIDE=0` was
        // served the translations an earlier run of the same
        // executable had cached with the switch unset.
        for name in [
            "VESPER_WAVE_WIDE",
            "VESPER_WAVE_DEEP_SKIPS",
            "VESPER_WAVE_OWN_BITS",
            "VESPER_WAVE_LOCAL",
            "VESPER_LDS_FLAGS",
            "VESPER_F16_NEAREST",
            "VESPER_HELPER_OWN",
            "VESPER_FIRSTLANE_OWN",
            "VESPER_FIRST_HALF",
            "VESPER_STRICT_JOINS",
            "VESPER_MESH_OUTPUT_STRICT",
            "VESPER_BVH_FENCES",
            "VESPER_BVH_BOUNDS",
        ] {
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(b'=');
            if let Some(value) = std::env::var_os(name) {
                bytes.extend_from_slice(value.as_encoded_bytes());
            }
            bytes.push(0);
        }
        // And which clean-up passes ran over the translated code.
        bytes.extend_from_slice(&vesper_shader::emit::shader_opt().to_le_bytes());
        // Renderer port 5 (`VESPER_PORT_CBV=1`): constant buffers read as raw
        // bytes. Only when on, so the translations cached without it keep
        // their key.
        if vesper_shader::emit::constants_raw() {
            bytes.extend_from_slice(b"VESPER_PORT_CBV=1");
        }
        bytes
    });
    let mut hash = blake3::Hasher::new();
    hash.update(fixed);
    for value in vesper_shader::emit::host_settings() {
        hash.update(&value.to_le_bytes());
    }
    // f16 arithmetic in native `half` (the host decides): only when on, so the
    // translations cached without it keep their key, and one path's entries
    // never serve the other.
    if vesper_shader::emit::host_native16() {
        hash.update(b"native16");
    }
    let hw_bvh = vesper_shader::emit::host_hw_bvh_opcode();
    if hw_bvh != 0 {
        hash.update(b"VESPER_HW_BVH=1");
        hash.update(&[hw_bvh]);
    }
    *hash.finalize().as_bytes()
}

/// What tells one set of translations from another ([`build`]).
pub(crate) fn translation() -> [u8; 32] {
    build()
}

fn key_for(container: &[u8]) -> (Option<u32>, [u8; 32]) {
    let own = own_loop_limit(container);
    // A shader with a loop limit of its own is another translation: only
    // its key changes, so no other shader is translated again.
    let key = match own {
        Some(limit) => {
            let mut h = blake3::Hasher::new();
            h.update(&build());
            h.update(b"loop limit of its own");
            h.update(&limit.to_le_bytes());
            *h.finalize().as_bytes()
        }
        None => build(),
    };
    (own, key)
}

/// Puts the translation of `container` into the cache before any title asks
/// for it (`vesper shaders`): whether it was there already, or why it does
/// not translate. The host is opened first, as a run does: what the card
/// can do decides the form of some programs.
pub fn precompile(container: &[u8]) -> Result<bool, String> {
    let _ = crate::host::host();
    let misses = MISSES.load(Ordering::Relaxed);
    let (own, key) = key_for(container);
    let translate = |c: &[u8]| {
        vesper_shader::emit::with_loop_limit(own, || vesper_shader::emit::translate(c))
            .map_err(|e| e.to_string())
    };
    match CACHE.get() {
        Some(cache) => cached(cache, &key, container, translate, true)?,
        None => translate(container)?,
    };
    // (Counts other threads' misses too: the caller only adds them up.)
    Ok(MISSES.load(Ordering::Relaxed) == misses)
}

/// The translation of a console shader container, from the cache when
/// there is one.
pub(crate) fn host_shader(container: &[u8]) -> Result<HostShader, String> {
    let (own, key) = key_for(container);
    let translate = |c: &[u8]| {
        vesper_shader::emit::with_loop_limit(own, || vesper_shader::emit::translate(c))
            .map_err(|e| e.to_string())
    };
    match CACHE.get() {
        Some(cache) => cached(cache, &key, container, translate, refresh_in_runs()),
        None => translate(container),
    }
}

/// The shaders whose loops stop sooner than their stage's limit, by the
/// hash the runtime names them by (16 hex digits of the container's
/// BLAKE3), as `hash:iterations` pairs: `VESPER_SHADER_LOOP_LIMITS`, empty
/// for none; unset, [`OWN_LOOP_LIMITS`].
const OWN_LOOP_LIMITS: &str = "39f0e9aeb156a61c:512";

/// [`OWN_LOOP_LIMITS`] for `container`.
fn own_loop_limit(container: &[u8]) -> Option<u32> {
    static LIMITS: OnceLock<Vec<(String, u32)>> = OnceLock::new();
    let limits = LIMITS.get_or_init(|| {
        let text = std::env::var("VESPER_SHADER_LOOP_LIMITS")
            .unwrap_or_else(|_| OWN_LOOP_LIMITS.to_owned());
        let limits = parse_loop_limits(&text);
        if !limits.is_empty() {
            tracing::info!(
                ?limits,
                "shaders whose loops stop sooner (VESPER_SHADER_LOOP_LIMITS)"
            );
        }
        limits
    });
    if limits.is_empty() {
        return None;
    }
    let hash = blake3::hash(container).to_hex();
    limits
        .iter()
        .find(|(h, _)| h.as_str() == &hash[..16])
        .map(|&(_, n)| n)
}

/// `hash:iterations` pairs, comma-separated; what is not one is left out.
fn parse_loop_limits(text: &str) -> Vec<(String, u32)> {
    text.split(',')
        .filter_map(|pair| {
            let (hash, n) = pair.trim().split_once(':')?;
            let hash = hash.trim().to_ascii_lowercase();
            let n = n.trim().parse::<u32>().ok().filter(|&n| n > 0)?;
            (hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some((hash, n))
        })
        .collect()
}

/// The cache entry's converter: the build and the container in, the
/// encoded translation out. `ran` tells a miss from a hit.
struct Translate<'a, F> {
    translate: F,
    /// The build of the translator, written into the entry.
    source: &'a str,
    ran: AtomicBool,
}

impl<F: Fn(&[u8]) -> Result<HostShader, String> + Send + Sync> Converter for Translate<'_, F> {
    fn id(&self) -> &'static str {
        "shader.dxil"
    }
    fn version(&self) -> u32 {
        // The build fingerprint in the input does the versioning.
        1
    }
    fn convert(&self, input: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.ran.store(true, Ordering::Relaxed);
        let shader = (self.translate)(&input[32..]).map_err(anyhow::Error::msg)?;
        Ok(encode(&shader, self.source))
    }
}

/// `refresh`: an entry an earlier build of the translator made is
/// translated again (and replaced) rather than served.
fn cached(
    cache: &Cache,
    build: &[u8; 32],
    container: &[u8],
    translate: impl Fn(&[u8]) -> Result<HostShader, String> + Send + Sync,
    refresh: bool,
) -> Result<HostShader, String> {
    cached_by(cache, build, SOURCE, container, translate, refresh)
}

/// [`cached`] with the build of the translator given (`source`).
fn cached_by(
    cache: &Cache,
    build: &[u8; 32],
    source: &str,
    container: &[u8],
    translate: impl Fn(&[u8]) -> Result<HostShader, String> + Send + Sync,
    refresh: bool,
) -> Result<HostShader, String> {
    let input = [&build[..], container].concat();
    let converter = Translate {
        translate,
        source,
        ran: AtomicBool::new(false),
    };
    let mut old: Option<HostShader> = None;
    for _ in 0..3 {
        let path = cache
            .get_or_convert(&converter, &input)
            .map_err(|e| format!("{e:#}"))?;
        if let Some((shader, made_by)) = std::fs::read(&path).ok().and_then(|b| decode(&b)) {
            let ran = converter.ran.load(Ordering::Relaxed);
            if !ran && made_by != source.as_bytes() {
                STALE.fetch_add(1, Ordering::Relaxed);
                if refresh && old.is_none() {
                    // Translated again in its place; what came out before
                    // is what the new one is compared with.
                    tracing::debug!(path = %path.display(), "shader cache entry of an earlier translator; translating again");
                    old = Some(shader);
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                static SAID: AtomicBool = AtomicBool::new(false);
                if !SAID.swap(true, Ordering::Relaxed) {
                    tracing::info!("shader translations of an earlier build of the translator (same version) are served; `vesper shaders` translates them again");
                }
            }
            if let Some(old) = old.take() {
                REFRESHED.fetch_add(1, Ordering::Relaxed);
                if old != shader {
                    CHANGED.fetch_add(1, Ordering::Relaxed);
                }
            }
            let (counter, what) = if ran {
                (&MISSES, "translated and cached")
            } else {
                (&HITS, "from the cache")
            };
            counter.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(path = %path.display(), "shader {what}");
            return Ok(shader);
        }
        tracing::debug!(path = %path.display(), "unreadable shader cache entry; translating again");
        let _ = std::fs::remove_file(&path);
    }
    MISSES.fetch_add(1, Ordering::Relaxed);
    (converter.translate)(container)
}

const MAGIC: &[u8; 4] = b"VSH2";
const STAGES: [HwStage; 8] = [
    HwStage::Ls,
    HwStage::Hs,
    HwStage::Es,
    HwStage::Gs,
    HwStage::Vs,
    HwStage::Ps,
    HwStage::Ms,
    HwStage::Cs,
];

fn root_code(r: RootType) -> u32 {
    match r {
        RootType::Constants => 0,
        RootType::Descriptor => 1,
        RootType::Table => 2,
        RootType::ImmediateSampler => 3,
        RootType::ResourceHeap => 4,
        RootType::SamplerHeap => 5,
        RootType::Other(x) => 6 + x,
    }
}

fn root_type(code: u32) -> RootType {
    match code {
        0 => RootType::Constants,
        1 => RootType::Descriptor,
        2 => RootType::Table,
        3 => RootType::ImmediateSampler,
        4 => RootType::ResourceHeap,
        5 => RootType::SamplerHeap,
        x => RootType::Other(x - 6),
    }
}

/// Little-endian words and length-prefixed lists, after the build of the
/// translator that made the entry (`source`, length-prefixed).
fn encode(s: &HostShader, source: &str) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    let word = |out: &mut Vec<u8>, v: u32| out.extend(v.to_le_bytes());
    word(&mut out, source.len() as u32);
    out.extend(source.as_bytes());
    let stage = STAGES.iter().position(|&x| x == s.stage).unwrap_or(0) as u32;
    word(&mut out, stage);
    word(&mut out, s.dxil.len() as u32);
    out.extend(&s.dxil);
    word(&mut out, s.user_data.len() as u32);
    for u in &s.user_data {
        for v in [u.index, u32::from(u.slot), root_code(u.root), u.dword] {
            word(&mut out, v);
        }
    }
    word(&mut out, s.samplers.len() as u32);
    for sampler in &s.samplers {
        for &v in sampler {
            word(&mut out, v);
        }
    }
    word(&mut out, s.static_samplers.len() as u32);
    for t in &s.static_samplers {
        let [u, v, w] = t.address;
        let words = [
            t.filter,
            u,
            v,
            w,
            t.mip_lod_bias.to_bits(),
            t.max_anisotropy,
            t.comparison,
            t.border,
            t.min_lod.to_bits(),
            t.max_lod.to_bits(),
        ];
        for v in words {
            word(&mut out, v);
        }
    }
    for slots in [&s.root_cbvs, &s.root_srvs, &s.root_uavs] {
        word(&mut out, slots.len() as u32);
        out.extend(slots);
    }
    word(&mut out, s.root_indexed_views.len() as u32);
    for view in &s.root_indexed_views {
        for value in [
            u32::from(view.table_slot),
            u32::from(view.constant_slot),
            view.constant_dword,
            view.byte_offset,
            u32::from(view.shift),
        ] {
            word(&mut out, value);
        }
    }
    word(&mut out, s.half_views.len() as u32);
    for &(slot, dword) in &s.half_views {
        word(&mut out, u32::from(slot));
        word(&mut out, dword);
    }
    out
}

/// The shader and the build of the translator that made it.
fn decode(bytes: &[u8]) -> Option<(HostShader, Vec<u8>)> {
    struct Reader<'a>(&'a [u8]);
    impl<'a> Reader<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let (head, rest) = self.0.split_at_checked(n)?;
            self.0 = rest;
            Some(head)
        }
        fn word(&mut self) -> Option<u32> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
        }
        fn list(&mut self) -> Option<Vec<u8>> {
            let n = self.word()? as usize;
            Some(self.take(n)?.to_vec())
        }
    }
    let mut r = Reader(bytes);
    if r.take(4)? != MAGIC {
        return None;
    }
    let made_by = r.list()?;
    let stage = *STAGES.get(r.word()? as usize)?;
    let dxil = r.list()?;
    let n = r.word()? as usize;
    let mut user_data = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        user_data.push(UserData {
            index: r.word()?,
            slot: u8::try_from(r.word()?).ok()?,
            root: root_type(r.word()?),
            dword: r.word()?,
        });
    }
    let n = r.word()? as usize;
    let mut samplers = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        samplers.push([r.word()?, r.word()?, r.word()?, r.word()?]);
    }
    let n = r.word()? as usize;
    let mut static_samplers = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        static_samplers.push(StaticSampler {
            filter: r.word()?,
            address: [r.word()?, r.word()?, r.word()?],
            mip_lod_bias: f32::from_bits(r.word()?),
            max_anisotropy: r.word()?,
            comparison: r.word()?,
            border: r.word()?,
            min_lod: f32::from_bits(r.word()?),
            max_lod: f32::from_bits(r.word()?),
        });
    }
    let root_cbvs = r.list()?;
    let root_srvs = r.list()?;
    let root_uavs = r.list()?;
    let n = r.word()? as usize;
    let mut root_indexed_views = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        root_indexed_views.push(RootIndexedView {
            table_slot: u8::try_from(r.word()?).ok()?,
            constant_slot: u8::try_from(r.word()?).ok()?,
            constant_dword: r.word()?,
            byte_offset: r.word()?,
            shift: u8::try_from(r.word()?).ok().filter(|&s| s < 32)?,
        });
    }
    let n = r.word()? as usize;
    let mut half_views = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        half_views.push((u8::try_from(r.word()?).ok()?, r.word()?));
    }
    r.0.is_empty().then_some((
        HostShader {
        stage,
        dxil,
        user_data,
        samplers,
        static_samplers,
        root_cbvs,
        root_srvs,
        root_uavs,
        root_indexed_views,
        half_views,
        },
        made_by,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_loop_limits_are_read_as_hash_and_count() {
        assert_eq!(
            parse_loop_limits(OWN_LOOP_LIMITS),
            [("39f0e9aeb156a61c".to_owned(), 512)]
        );
        assert_eq!(
            parse_loop_limits(
                " 39F0E9AEB156A61C : 64 ,deadbeef:5, 0123456789abcdef:0, 0123456789abcdef:12,x"
            ),
            [
                ("39f0e9aeb156a61c".to_owned(), 64),
                ("0123456789abcdef".to_owned(), 12)
            ]
        );
        assert!(parse_loop_limits("").is_empty());
        let container = b"DXBC not a real container";
        let hash = blake3::hash(container).to_hex()[..16].to_owned();
        assert!(
            parse_loop_limits(&format!("{hash}:9"))
                .iter()
                .any(|(h, n)| *h == hash && *n == 9)
        );
    }

    fn shader() -> HostShader {
        HostShader {
            stage: HwStage::Ps,
            dxil: b"DXBC and the rest".to_vec(),
            user_data: vec![UserData {
                index: 8,
                slot: 1,
                root: RootType::Table,
                dword: 0,
            }],
            samplers: vec![[0x400, 0xc0_0000, 0x18f0_0000, 0]],
            static_samplers: vec![StaticSampler::from_words([
                0x400,
                0xc0_0000,
                0x18f0_0000,
                0,
            ])],
            root_cbvs: vec![2],
            root_srvs: vec![0, 3],
            root_uavs: vec![1],
            root_indexed_views: vec![RootIndexedView {
                table_slot: 1,
                constant_slot: 2,
                constant_dword: 3,
                byte_offset: 32,
                shift: 5,
            }],
            half_views: vec![(0, 8), (4, 0)],
        }
    }

    #[test]
    fn translations_come_back_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path()).unwrap();
        let build = [7u8; 32];
        let container = b"a console container".as_slice();
        let runs = AtomicU64::new(0);
        let translate = |_: &[u8]| {
            runs.fetch_add(1, Ordering::Relaxed);
            Ok(shader())
        };
        assert_eq!(cached(&cache, &build, container, translate, false), Ok(shader()));
        assert_eq!(cached(&cache, &build, container, translate, false), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 1, "the second came from disk");

        // Another build translates again.
        assert_eq!(cached(&cache, &[8; 32], container, translate, false), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 2);

        // A damaged entry is a miss, translated and stored again.
        for e in walk(dir.path()) {
            std::fs::write(e, b"VSH2 broken").unwrap();
        }
        assert_eq!(cached(&cache, &build, container, translate, false), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 3);
        assert_eq!(cached(&cache, &build, container, translate, false), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 3);

        // Failures are not stored.
        let fail = |_: &[u8]| Err::<HostShader, _>("unsupported".to_owned());
        assert!(cached(&cache, &build, b"other", fail, false).is_err());
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.path().is_dir() {
                files.extend(walk(&e.path()));
            } else {
                files.push(e.path());
            }
        }
        files
    }

    #[test]
    fn entries_round_trip_and_reject_damage() {
        let bytes = encode(&shader(), "made by");
        assert_eq!(decode(&bytes), Some((shader(), b"made by".to_vec())));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode(&[bytes.as_slice(), &[0]].concat()), None);
        assert_eq!(decode(b"VSH0"), None);
        // Entries of the encoding before the translator's build was in them.
        let mut v1 = bytes.clone();
        v1[3] = b'1';
        assert_eq!(decode(&v1), None);
    }

    /// An entry an earlier build of the translator made, under the same
    /// version: served as it is; when refreshing translated again in its
    /// place, and a different result counted.
    #[test]
    fn entries_of_an_earlier_translator_are_served_or_refreshed() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path()).unwrap();
        let build = [9u8; 32];
        let container = b"a container of an earlier build".as_slice();
        let runs = AtomicU64::new(0);
        let old = || {
            let mut s = shader();
            s.dxil = b"the old translation".to_vec();
            s
        };
        let first = |_: &[u8]| {
            runs.fetch_add(1, Ordering::Relaxed);
            Ok(old())
        };
        assert_eq!(cached_by(&cache, &build, "build a", container, first, false), Ok(old()));
        let now = |_: &[u8]| {
            runs.fetch_add(1, Ordering::Relaxed);
            Ok(shader())
        };
        let (stale, refreshed, changed) = stale_counts();
        // The next build, same version: the old translation, not translated.
        assert_eq!(cached_by(&cache, &build, "build b", container, now, false), Ok(old()));
        assert_eq!(runs.load(Ordering::Relaxed), 1);
        // Refreshing: translated again and kept.
        assert_eq!(cached_by(&cache, &build, "build b", container, now, true), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 2);
        // Now the entry is this build's: served when refreshing too.
        assert_eq!(cached_by(&cache, &build, "build b", container, now, true), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 2);
        // (Other tests count too; these at least.)
        let (s, r, c) = stale_counts();
        assert!(s >= stale + 2 && r > refreshed && c > changed, "{s} {r} {c}");
    }
}
