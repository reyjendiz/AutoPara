//! The whole pipeline on synthetic audio: fake source, scripted recogniser, canned chat replies.

use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use lecture_assist::audio::AudioSource;
use lecture_assist::audio::fake::FakeSource;
use lecture_assist::llm::{ChatProvider, Message, ReplyStream};
use lecture_assist::stt::{Recognizer, Scripted};
use lecture_assist::vad::{EnergyVad, Vad};
use lecture_assist::{
    AskKind, AssistConfig, AssistEngine, AssistError, AssistEvent, ClassContext, FlagKind, Parts,
    SAMPLE_RATE, State,
};

const NOTES: &str = "{\"topic\": \"Графи\", \"key_points\": [\"Граф — вершини і ребра\"], \
    \"terms\": [{\"term\": \"Ребро\", \"explanation\": \"зв’язок двох вершин\"}], \
    \"summary\": \"Вступ до графів.\"}";

/// Notes requests get JSON, helpers get plain text, in two pieces.
struct Canned;

impl ChatProvider for Canned {
    fn stream(&self, messages: Vec<Message>) -> BoxFuture<'_, Result<ReplyStream, AssistError>> {
        let notes = messages[0].content.contains("JSON object");
        let pieces: Vec<Result<String, AssistError>> = if notes {
            vec![Ok(NOTES.into())]
        } else {
            vec![Ok("# Графи\n".into()), Ok("- вершини".into())]
        };
        Box::pin(async move { Ok(futures_util::stream::iter(pieces).boxed()) })
    }
}

/// Silence, then a phrase of `seconds` of tone, for each phrase.
fn lecture(phrases: &[f32]) -> Vec<f32> {
    let second = SAMPLE_RATE as usize;
    let mut out = Vec::new();
    for &seconds in phrases {
        out.extend(vec![0.0; second]);
        let n = (seconds * SAMPLE_RATE as f32) as usize;
        out.extend((0..n).map(|i| (i as f32 * 0.08).sin() * 0.3));
    }
    out.extend(vec![0.0; 2 * second]);
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn audio_becomes_transcript_flags_notes_and_a_summary() {
    let script = [
        "Сьогодні говоримо про графи.",
        "Домашнє завдання: вправа три.",
    ];
    let parts = Parts {
        source: Box::new(|| {
            Ok(Box::new(FakeSource::new(lecture(&[2.0, 2.0]), 1600)) as Box<dyn AudioSource>)
        }),
        recognizer: Box::new(move || {
            let lines = script.iter().map(|s| s.to_string()).collect();
            Ok(Box::new(Scripted(lines)) as Box<dyn Recognizer>)
        }),
        vad: Box::new(|| Ok(Box::new(EnergyVad::default()) as Box<dyn Vad>)),
        chat: Some(Arc::new(Canned)),
    };
    let config = AssistConfig {
        notes_every_secs: 3,
        partial_every_ms: 0,
        ..AssistConfig::default()
    };
    let context = ClassContext {
        subject: "Дискретна математика".into(),
        ..ClassContext::default()
    };
    let (handle, mut events) = AssistEngine::start(config, context, parts);

    let mut states = Vec::new();
    let mut finals = Vec::new();
    let mut flags = Vec::new();
    let mut topics = Vec::new();
    let mut terms = Vec::new();
    let mut summary = String::new();
    while let Some(event) = events.recv().await {
        match event {
            AssistEvent::StateChanged { state } => {
                let off = state == State::Off;
                states.push(state);
                if off {
                    break;
                }
            }
            AssistEvent::TranscriptFinal { segment } => finals.push(segment.text),
            AssistEvent::Flagged { segment } => flags.push(segment.flag),
            AssistEvent::NotesUpdated { notes } => topics.push(notes.topic),
            AssistEvent::TermsUpdated { terms: list } => terms = list,
            AssistEvent::HelperReply { kind, text, .. } => {
                assert_eq!(kind, AskKind::Summary);
                summary.push_str(&text);
            }
            AssistEvent::Error { code, detail } => panic!("{code}: {detail}"),
            _ => {}
        }
    }
    drop(handle);

    assert_eq!(
        states,
        [
            State::Starting,
            State::Listening,
            State::Stopping,
            State::Off
        ]
    );
    assert_eq!(finals, script);
    assert_eq!(flags, [Some(FlagKind::Task)]);
    assert!(topics.contains(&"Графи".to_string()), "{topics:?}");
    assert_eq!(terms.len(), 1);
    assert_eq!(summary, "# Графи\n- вершини");
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_and_resume_report_their_states() {
    let parts = Parts {
        // Endless silence, so only the commands end it.
        source: Box::new(|| {
            Ok(Box::new(FakeSource::new(vec![0.0; 16_000 * 600], 1600)) as Box<dyn AudioSource>)
        }),
        recognizer: Box::new(|| Ok(Box::new(Scripted(Default::default())) as Box<dyn Recognizer>)),
        vad: Box::new(|| Ok(Box::new(EnergyVad::default()) as Box<dyn Vad>)),
        chat: None,
    };
    let (handle, mut events) =
        AssistEngine::start(AssistConfig::default(), ClassContext::default(), parts);
    handle.pause();
    handle.resume();
    handle.stop();
    let mut states = Vec::new();
    while let Some(event) = events.recv().await {
        if let AssistEvent::StateChanged { state } = event {
            let off = state == State::Off;
            states.push(state);
            if off {
                break;
            }
        }
    }
    assert_eq!(
        states,
        [
            State::Starting,
            State::Listening,
            State::Paused,
            State::Listening,
            State::Stopping,
            State::Off
        ]
    );
}
