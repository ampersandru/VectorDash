// C ABI for pinned sherpa-onnx 1.13.8, generated from its bundled c-api.h.
// Copyright Xiaomi Corporation; Apache-2.0 (see Parakeet-NOTICE.txt).
#![allow(non_snake_case, dead_code)]
use anyhow::{Context, Result};
use std::ffi::{CStr, CString, c_char, c_void};

#[repr(C)]
pub struct SherpaOnnxFeatureConfig {
    pub sample_rate: i32,
    pub feature_dim: i32,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTransducerModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub joiner: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineParaformerModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineNemoEncDecCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineWhisperModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub language: *const c_char,
    pub task: *const c_char,
    pub tail_paddings: i32,
    pub enable_token_timestamps: i32,
    pub enable_segment_timestamps: i32,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTdnnModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineSenseVoiceModelConfig {
    pub model: *const c_char,
    pub language: *const c_char,
    pub use_itn: i32,
}

#[repr(C)]
pub struct SherpaOnnxOfflineMoonshineModelConfig {
    pub preprocessor: *const c_char,
    pub encoder: *const c_char,
    pub uncached_decoder: *const c_char,
    pub cached_decoder: *const c_char,
    pub merged_decoder: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineFireRedAsrModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineDolphinModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineZipformerCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineCanaryModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub src_lang: *const c_char,
    pub tgt_lang: *const c_char,
    pub use_pnc: i32,
}

#[repr(C)]
pub struct SherpaOnnxOfflineWenetCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineOmnilingualAsrCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineMedAsrCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineFunASRNanoModelConfig {
    pub encoder_adaptor: *const c_char,
    pub llm: *const c_char,
    pub embedding: *const c_char,
    pub tokenizer: *const c_char,
    pub system_prompt: *const c_char,
    pub user_prompt: *const c_char,
    pub max_new_tokens: i32,
    pub temperature: f32,
    pub top_p: f32,
    pub seed: i32,
    pub language: *const c_char,
    pub itn: i32,
    pub hotwords: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineFireRedAsrCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineQwen3ASRModelConfig {
    pub conv_frontend: *const c_char,
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub tokenizer: *const c_char,
    pub max_total_len: i32,
    pub max_new_tokens: i32,
    pub temperature: f32,
    pub top_p: f32,
    pub seed: i32,
    pub hotwords: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineCohereTranscribeModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub language: *const c_char,
    pub use_punct: i32,
    pub use_itn: i32,
}

#[repr(C)]
pub struct SherpaOnnxOfflineModelConfig {
    pub transducer: SherpaOnnxOfflineTransducerModelConfig,
    pub paraformer: SherpaOnnxOfflineParaformerModelConfig,
    pub nemo_ctc: SherpaOnnxOfflineNemoEncDecCtcModelConfig,
    pub whisper: SherpaOnnxOfflineWhisperModelConfig,
    pub tdnn: SherpaOnnxOfflineTdnnModelConfig,
    pub tokens: *const c_char,
    pub num_threads: i32,
    pub debug: i32,
    pub provider: *const c_char,
    pub model_type: *const c_char,
    pub modeling_unit: *const c_char,
    pub bpe_vocab: *const c_char,
    pub telespeech_ctc: *const c_char,
    pub sense_voice: SherpaOnnxOfflineSenseVoiceModelConfig,
    pub moonshine: SherpaOnnxOfflineMoonshineModelConfig,
    pub fire_red_asr: SherpaOnnxOfflineFireRedAsrModelConfig,
    pub dolphin: SherpaOnnxOfflineDolphinModelConfig,
    pub zipformer_ctc: SherpaOnnxOfflineZipformerCtcModelConfig,
    pub canary: SherpaOnnxOfflineCanaryModelConfig,
    pub wenet_ctc: SherpaOnnxOfflineWenetCtcModelConfig,
    pub omnilingual: SherpaOnnxOfflineOmnilingualAsrCtcModelConfig,
    pub medasr: SherpaOnnxOfflineMedAsrCtcModelConfig,
    pub funasr_nano: SherpaOnnxOfflineFunASRNanoModelConfig,
    pub fire_red_asr_ctc: SherpaOnnxOfflineFireRedAsrCtcModelConfig,
    pub qwen3_asr: SherpaOnnxOfflineQwen3ASRModelConfig,
    pub cohere_transcribe: SherpaOnnxOfflineCohereTranscribeModelConfig,
}

#[repr(C)]
pub struct SherpaOnnxOfflineLMConfig {
    pub model: *const c_char,
    pub scale: f32,
}

#[repr(C)]
pub struct SherpaOnnxHomophoneReplacerConfig {
    pub dict_dir: *const c_char,
    pub lexicon: *const c_char,
    pub rule_fsts: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineRecognizerConfig {
    pub feat_config: SherpaOnnxFeatureConfig,
    pub model_config: SherpaOnnxOfflineModelConfig,
    pub lm_config: SherpaOnnxOfflineLMConfig,
    pub decoding_method: *const c_char,
    pub max_active_paths: i32,
    pub hotwords_file: *const c_char,
    pub hotwords_score: f32,
    pub rule_fsts: *const c_char,
    pub rule_fars: *const c_char,
    pub blank_penalty: f32,
    pub hr: SherpaOnnxHomophoneReplacerConfig,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct WorkerConfig {
    pub library: String,
    pub encoder: String,
    pub decoder: String,
    pub joiner: String,
    pub tokens: String,
    pub provider: String,
}
pub fn worker() -> Result<()> {
    use std::io::{BufRead, Write};
    let paths: WorkerConfig = serde_json::from_str(&std::env::var("VECTORDASH_SPEECH_CONFIG")?)?;
    // The worker is an owned child process. Crashes/cancellation cannot corrupt the GUI.
    unsafe {
        #[cfg(windows)]
        let library: libloading::Library =
            libloading::os::windows::Library::load_with_flags(&paths.library, 0x00000008)?.into();
        #[cfg(not(windows))]
        let library = libloading::Library::new(&paths.library)?;
        let version =
            library.get::<unsafe extern "C" fn() -> *const c_char>(b"SherpaOnnxGetVersionStr\0")?;
        anyhow::ensure!(
            CStr::from_ptr(version())
                .to_string_lossy()
                .contains("1.13.8"),
            "Unsupported speech runtime ABI"
        );
        let create = library.get::<unsafe extern "C" fn(
            *const SherpaOnnxOfflineRecognizerConfig,
        ) -> *const c_void>(b"SherpaOnnxCreateOfflineRecognizer\0")?;
        let destroy = library
            .get::<unsafe extern "C" fn(*const c_void)>(b"SherpaOnnxDestroyOfflineRecognizer\0")?;
        let stream_create = library.get::<unsafe extern "C" fn(*const c_void) -> *const c_void>(
            b"SherpaOnnxCreateOfflineStream\0",
        )?;
        let stream_destroy = library
            .get::<unsafe extern "C" fn(*const c_void)>(b"SherpaOnnxDestroyOfflineStream\0")?;
        let accept = library.get::<unsafe extern "C" fn(*const c_void, i32, *const f32, i32)>(
            b"SherpaOnnxAcceptWaveformOffline\0",
        )?;
        let decode = library.get::<unsafe extern "C" fn(*const c_void, *const c_void)>(
            b"SherpaOnnxDecodeOfflineStream\0",
        )?;
        let result = library.get::<unsafe extern "C" fn(*const c_void) -> *const c_char>(
            b"SherpaOnnxGetOfflineStreamResultAsJson\0",
        )?;
        let free = library.get::<unsafe extern "C" fn(*const c_char)>(
            b"SherpaOnnxDestroyOfflineStreamResultJson\0",
        )?;
        let strings = [
            paths.encoder,
            paths.decoder,
            paths.joiner,
            paths.tokens,
            paths.provider,
            "nemo_transducer".into(),
            "greedy_search".into(),
        ]
        .into_iter()
        .map(CString::new)
        .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut config: SherpaOnnxOfflineRecognizerConfig = std::mem::zeroed();
        config.feat_config.sample_rate = 16000;
        config.feat_config.feature_dim = 80;
        config.model_config.transducer.encoder = strings[0].as_ptr();
        config.model_config.transducer.decoder = strings[1].as_ptr();
        config.model_config.transducer.joiner = strings[2].as_ptr();
        config.model_config.tokens = strings[3].as_ptr();
        config.model_config.provider = strings[4].as_ptr();
        config.model_config.model_type = strings[5].as_ptr();
        config.model_config.num_threads = 2;
        config.decoding_method = strings[6].as_ptr();
        config.max_active_paths = 4;
        let recognizer = create(&config);
        anyhow::ensure!(!recognizer.is_null(), "Could not load Parakeet model");
        println!("{{\"ready\":true}}");
        std::io::stdout().flush()?;
        for line in std::io::stdin().lock().lines() {
            let answer = (|| -> Result<serde_json::Value> {
                let line = line?;
                anyhow::ensure!(line.len() < 32768, "Invalid speech request");
                let request: serde_json::Value = serde_json::from_str(&line)?;
                let path = request["wav"]
                    .as_str()
                    .context("Missing speech recording")?;
                let mut wav = hound::WavReader::open(path)?;
                let spec = wav.spec();
                anyhow::ensure!(
                    spec.channels == 1 && spec.sample_rate == 16000 && wav.duration() <= 16000 * 60,
                    "Unsupported speech recording"
                );
                let samples = wav
                    .samples::<i16>()
                    .map(|v| v.map(|n| n as f32 / 32768.0))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let stream = stream_create(recognizer);
                anyhow::ensure!(!stream.is_null(), "Could not create speech stream");
                accept(stream, 16000, samples.as_ptr(), samples.len() as i32);
                decode(recognizer, stream);
                let json = result(stream);
                let answer = if json.is_null() {
                    Err(anyhow::anyhow!("Empty speech result"))
                } else {
                    serde_json::from_str::<serde_json::Value>(
                        &CStr::from_ptr(json).to_string_lossy(),
                    )
                    .map_err(anyhow::Error::from)
                };
                if !json.is_null() {
                    free(json);
                }
                stream_destroy(stream);
                answer
            })()
            .unwrap_or_else(|e| serde_json::json!({"error":e.to_string()}));
            println!("{answer}");
            std::io::stdout().flush()?;
        }
        destroy(recognizer);
    }
    Ok(())
}
