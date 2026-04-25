//! Bundled starter abbreviation dictionary.

use super::model::{Abbreviation, Trigger, TriggerEvent};

#[allow(dead_code)]
mod cho {
    pub const G:  u32 = 0x1100; // ㄱ
    pub const GG: u32 = 0x1101; // ㄲ
    pub const N:  u32 = 0x1102; // ㄴ
    pub const D:  u32 = 0x1103; // ㄷ
    pub const DD: u32 = 0x1104; // ㄸ
    pub const R:  u32 = 0x1105; // ㄹ
    pub const M:  u32 = 0x1106; // ㅁ
    pub const B:  u32 = 0x1107; // ㅂ
    pub const BB: u32 = 0x1108; // ㅃ
    pub const S:  u32 = 0x1109; // ㅅ
    pub const SS: u32 = 0x110A; // ㅆ
    pub const O:  u32 = 0x110B; // ㅇ
    pub const J:  u32 = 0x110C; // ㅈ
    pub const JJ: u32 = 0x110D; // ㅉ
    pub const C:  u32 = 0x110E; // ㅊ
    pub const K:  u32 = 0x110F; // ㅋ
    pub const T:  u32 = 0x1110; // ㅌ
    pub const P:  u32 = 0x1111; // ㅍ
    pub const H:  u32 = 0x1112; // ㅎ
}
use cho::{B, C, D, DD, G, GG, H, J, K, M, N, O, R, S, T};

// Priority tiers used by the starter dictionary — higher = surfaces
// first among otherwise-equal matches.
const PRI_COMMON:   u32 = 100;
const PRI_POLITE:   u32 = 80;
const PRI_WORD:     u32 = 70;
const PRI_RARE:     u32 = 60;
const PRI_QUESTION: u32 = 50;

fn cho_abbr(id: &str, chos: &[u32], body: &str, ev: TriggerEvent) -> Abbreviation {
    Abbreviation {
        id: id.into(),
        trigger: Trigger::ChoSeq(chos.to_vec()),
        body: body.into(),
        trigger_on: ev,
        priority: PRI_COMMON,
    }
}

/// 어미 자동완성용 cho_seq 항목. 사용자가 Tab 으로 명시적으로 발화한다.
/// `id` 는 `end-cho-…` prefix 로 식별된다.
fn cho_end(suffix: &str, chos: &[u32], body: &str, priority: u32) -> Abbreviation {
    Abbreviation {
        id: format!("end-cho-{suffix}"),
        trigger: Trigger::ChoSeq(chos.to_vec()),
        body: body.into(),
        trigger_on: TriggerEvent::Explicit,
        priority,
    }
}

fn lit(id: &str, text: &str, body: &str, ev: TriggerEvent) -> Abbreviation {
    Abbreviation {
        id: id.into(),
        trigger: Trigger::Literal(text.into()),
        body: body.into(),
        trigger_on: ev,
        priority: PRI_COMMON,
    }
}

fn ending_p(text: &str, body: &str, priority: u32) -> Abbreviation {
    Abbreviation {
        id: format!("end:{text}"),
        trigger: Trigger::Ending(text.into()),
        body: body.into(),
        trigger_on: TriggerEvent::Explicit,
        priority,
    }
}

/// A conjunction that can be prefix-matched as a literal word AND via
/// its sequence of initial consonants. Both triggers share the same
/// priority tier.
fn conj(word: &str, chos: &[u32]) -> [Abbreviation; 2] {
    [
        Abbreviation {
            id: format!("conj-lit:{word}"),
            trigger: Trigger::Literal(word.into()),
            body: word.into(),
            trigger_on: TriggerEvent::Explicit,
            priority: PRI_COMMON,
        },
        Abbreviation {
            id: format!("conj-cho:{word}"),
            trigger: Trigger::ChoSeq(chos.to_vec()),
            body: word.into(),
            trigger_on: TriggerEvent::Explicit,
            priority: PRI_COMMON,
        },
    ]
}

/// 한글 음절 문자열에서 초성(U+1100 ~ U+1112) 코드포인트 시퀀스를
/// 추출한다. 한글 이외의 문자는 건너뛴다.
/// 예: "사람" → [ㅅ(0x1109), ㄹ(0x1105)]
fn chos_of(word: &str) -> Vec<u32> {
    word.chars()
        .filter_map(|c| {
            let cp = c as u32;
            if (0xAC00..=0xD7A3).contains(&cp) {
                Some(0x1100 + (cp - 0xAC00) / (21 * 28))
            } else {
                None
            }
        })
        .collect()
}

/// 자주 쓰는 낱말을 literal(단어 경계 매칭) + cho_seq(초성 시퀀스) 두
/// 트리거로 동시에 등록한다. 우선순위는 PRI_WORD(70) — 인사말·어미·
/// 접속사(PRI_COMMON=100 / PRI_POLITE=80) 아래에 자리한다.
fn word_entry(word: &str) -> [Abbreviation; 2] {
    [
        Abbreviation {
            id: format!("word-lit:{word}"),
            trigger: Trigger::Literal(word.into()),
            body: word.into(),
            trigger_on: TriggerEvent::Explicit,
            priority: PRI_WORD,
        },
        Abbreviation {
            id: format!("word-cho:{word}"),
            trigger: Trigger::ChoSeq(chos_of(word)),
            body: word.into(),
            trigger_on: TriggerEvent::Explicit,
            priority: PRI_WORD,
        },
    ]
}

/// Returns the bundled starter set.
#[allow(clippy::too_many_lines)]
pub fn starter_dict() -> Vec<Abbreviation> {
    use TriggerEvent::Space;
    let mut v: Vec<Abbreviation> = Vec::new();

    // ── 인사/기본 (초성, 마침표 포함) ──────────────────────────────
    v.extend([
        cho_abbr("gs",    &[G, S],           "감사합니다.",       Space),
        cho_abbr("onhsy", &[O, N, H, S, O],  "안녕하세요.",       Space),
        cho_abbr("sg",    &[S, G],           "수고하셨습니다.",    Space),
        cho_abbr("jshnd", &[J, S, H, N, D],  "죄송합니다.",       Space),
        cho_abbr("chk",   &[C, K],           "축하합니다.",       Space),
        cho_abbr("ok",    &[O, K],           "알겠습니다.",       Space),
        cho_abbr("gd",    &[G, D],           "고생하셨습니다.",    Space),
        cho_abbr("hy",    &[H, O],           "환영합니다.",       Space),
        cho_abbr("jh",    &[J, H],           "잘 부탁드립니다.",   Space),
        cho_abbr("oo",    &[O, O],           "네, 확인했습니다.",  Space),
        cho_abbr("gt",    &[G, T],           "검토 부탁드립니다.", Space),
        cho_abbr("jhs",   &[J, H, S],        "좋은 하루 되세요.",  Space),
    ]);

    // ── 어미 (Ending) ─────────────────────────────────────────────
    // 선언형이 의문형보다 훨씬 자주 쓰이므로 우선순위를 높게 둡니다.
    v.extend([
        // Formal declarative — most common
        ending_p("습니다",   "습니다.",   PRI_COMMON),
        ending_p("입니다",   "입니다.",   PRI_COMMON),
        ending_p("합니다",   "합니다.",   PRI_COMMON),
        ending_p("됩니다",   "됩니다.",   PRI_COMMON),
        ending_p("있습니다", "있습니다.", PRI_COMMON),
        ending_p("없습니다", "없습니다.", PRI_COMMON),
        ending_p("드립니다", "드립니다.", PRI_COMMON),
        ending_p("겠습니다", "겠습니다.", PRI_COMMON),
        ending_p("였습니다", "였습니다.", PRI_COMMON),
        ending_p("었습니다", "었습니다.", PRI_COMMON),
        // Interrogative — less common, shown after declaratives
        ending_p("습니까",   "습니까?",   PRI_QUESTION),
        ending_p("입니까",   "입니까?",   PRI_QUESTION),
        ending_p("합니까",   "합니까?",   PRI_QUESTION),
        ending_p("됩니까",   "됩니까?",   PRI_QUESTION),
        ending_p("있습니까", "있습니까?", PRI_QUESTION),
        ending_p("없습니까", "없습니까?", PRI_QUESTION),
        ending_p("드립니까", "드립니까?", PRI_QUESTION),
        ending_p("겠습니까", "겠습니까?", PRI_QUESTION),
        ending_p("였습니까", "였습니까?", PRI_QUESTION),
        ending_p("었습니까", "었습니까?", PRI_QUESTION),
        // Polite declarative / imperative / request
        ending_p("주세요",   "주세요.",   PRI_COMMON),
        ending_p("세요",     "세요.",     PRI_COMMON),
        ending_p("해요",     "해요.",     PRI_COMMON),
        ending_p("해주세요", "해주세요.", PRI_COMMON),
        ending_p("어요",     "어요.",     PRI_POLITE),
        ending_p("아요",     "아요.",     PRI_POLITE),
        ending_p("네요",     "네요.",     PRI_POLITE),
        ending_p("군요",     "군요.",     PRI_POLITE),
        ending_p("죠",       "죠.",       PRI_POLITE),
        ending_p("드려요",   "드려요.",   PRI_POLITE),
        ending_p("드릴게요", "드릴게요.", PRI_RARE),
        // Interrogative "~나요 / 까요"
        ending_p("되나요",   "되나요?",   PRI_QUESTION),
        ending_p("될까요",   "될까요?",   PRI_QUESTION),
        ending_p("일까요",   "일까요?",   PRI_QUESTION),
        // Formal imperative
        ending_p("십시오",   "십시오.",   PRI_POLITE),
        ending_p("십시다",   "십시다.",   PRI_RARE),
    ]);

    // ── 어미 (ChoSeq) — 초성 입력 후 Tab 으로 발화 ─────────────────
    // 위 Ending 항목들과 짝을 이루는 cho_seq alias. 사용자가 본문을
    // 다 적기 전에 "ㅅㄴㄷ" 같은 초성만으로 빠르게 어미를 완성할 수
    // 있다. 의문형(까?)은 PRI_QUESTION 으로 한 단계 낮춰 선언형(다.)
    // 보다 뒤에 표시되도록 한다.
    v.extend([
        // ── 선언형 (~다.) ──
        cho_end("snd",     &[S, N, D],            "습니다.",    PRI_COMMON),
        cho_end("ond",     &[O, N, D],            "입니다.",    PRI_COMMON),
        cho_end("hnd",     &[H, N, D],            "합니다.",    PRI_COMMON),
        cho_end("dnd",     &[D, N, D],            "됩니다.",    PRI_COMMON),
        cho_end("isnd",    &[O, S, N, D],         "있습니다.",  PRI_COMMON),
        cho_end("eosnd",   &[O, S, N, D],         "없습니다.",  PRI_COMMON),
        cho_end("yeosnd",  &[O, S, N, D],         "였습니다.",  PRI_RARE),
        cho_end("eonsnd",  &[O, S, N, D],         "었습니다.",  PRI_RARE),
        cho_end("drnd",    &[D, R, N, D],         "드립니다.",  PRI_COMMON),
        cho_end("gsnd",    &[G, S, N, D],         "겠습니다.",  PRI_COMMON),
        cho_end("hsnd",    &[H, S, N, D],         "했습니다.",  PRI_COMMON),
        cho_end("jsnd",    &[J, S, N, D],         "좋습니다.",  PRI_COMMON),
        cho_end("gtsnd",   &[G, S, N, D],         "같습니다.",  PRI_RARE),
        cho_end("grsnd",   &[G, R, S, N, D],      "그렇습니다.", PRI_COMMON),
        cho_end("ogsnd",   &[O, G, S, N, D],      "알겠습니다.", PRI_COMMON),
        cho_end("mrgsnd",  &[M, R, G, S, N, D],   "모르겠습니다.", PRI_COMMON),
        cho_end("bnd",     &[B, N, D],            "봅니다.",    PRI_POLITE),
        cho_end("gnd",     &[G, N, D],            "갑니다.",    PRI_POLITE),
        cho_end("jund",    &[J, N, D],            "줍니다.",    PRI_POLITE),
        cho_end("bsnd",    &[B, S, N, D],         "받습니다.",  PRI_POLITE),
        cho_end("bond",    &[B, O, N, D],         "보입니다.",  PRI_POLITE),
        cho_end("bjnd",    &[B, J, N, D],         "바랍니다.",  PRI_COMMON),
        cho_end("hgsnd",   &[H, G, S, N, D],      "하겠습니다.", PRI_POLITE),
        cho_end("bthnd",   &[B, T, H, N, D],      "부탁합니다.", PRI_COMMON),
        cho_end("btdrnd",  &[B, T, D, R, N, D],   "부탁드립니다.", PRI_COMMON),

        // ── 의문형 (~까?) ── — 까 의 초성은 ㄲ(GG). 모두 PRI_QUESTION.
        cho_end("sng",     &[S, N, GG],           "습니까?",    PRI_QUESTION),
        cho_end("ong",     &[O, N, GG],           "입니까?",    PRI_QUESTION),
        cho_end("hng",     &[H, N, GG],           "합니까?",    PRI_QUESTION),
        cho_end("dng",     &[D, N, GG],           "됩니까?",    PRI_QUESTION),
        cho_end("isng",    &[O, S, N, GG],        "있습니까?",  PRI_QUESTION),
        cho_end("eosng",   &[O, S, N, GG],        "없습니까?",  PRI_QUESTION),
        cho_end("drng",    &[D, R, N, GG],        "드립니까?",  PRI_QUESTION),
        cho_end("gsng",    &[G, S, N, GG],        "겠습니까?",  PRI_QUESTION),
        cho_end("hsng",    &[H, S, N, GG],        "했습니까?",  PRI_QUESTION),
        cho_end("dnyo",    &[D, N, O],            "되나요?",    PRI_QUESTION),
        cho_end("dggyo",   &[D, GG, O],           "될까요?",    PRI_QUESTION),
        cho_end("oggyo",   &[O, GG, O],           "일까요?",    PRI_QUESTION),
        cho_end("grggyo",  &[G, R, GG, O],        "그럴까요?",  PRI_QUESTION),

        // ── 정중 비격식 (~요.) ──
        cho_end("hyo",     &[H, O],               "해요.",      PRI_POLITE),
        cho_end("syo",     &[S, O],               "세요.",      PRI_POLITE),
        cho_end("nyo",     &[N, O],               "네요.",      PRI_POLITE),
        cho_end("gyo",     &[G, O],               "군요.",      PRI_POLITE),
        cho_end("dryo",    &[D, R, O],            "드려요.",    PRI_POLITE),
        cho_end("drgyo",   &[D, R, G, O],         "드릴게요.",  PRI_RARE),
        cho_end("hoyo",    &[H, O, O],            "했어요.",    PRI_POLITE),
        cho_end("doyo",    &[D, O, O],            "됐어요.",    PRI_POLITE),
        cho_end("goyo",    &[G, O, O],            "갔어요.",    PRI_POLITE),
        cho_end("boyo",    &[B, O, O],            "봤어요.",    PRI_POLITE),
        cho_end("moyo",    &[M, O, O],            "맞아요.",    PRI_POLITE),
        cho_end("joyo",    &[J, O, O],            "좋아요.",    PRI_POLITE),
        cho_end("soyo",    &[S, O, O],            "싫어요.",    PRI_POLITE),
        cho_end("ayo",     &[O, O, O],            "알아요.",    PRI_RARE),
        cho_end("gryo",    &[G, R, O],            "그래요.",    PRI_POLITE),
        cho_end("mryo",    &[M, R, O],            "몰라요.",    PRI_POLITE),

        // ── 요청 (~주세요.) ──
        cho_end("jsyo",    &[J, S, O],            "주세요.",    PRI_COMMON),
        cho_end("hjsyo",   &[H, J, S, O],         "해주세요.",  PRI_COMMON),
        cho_end("dojsyo",  &[D, O, J, S, O],      "도와주세요.", PRI_COMMON),
        cho_end("orjsyo",  &[O, R, J, S, O],      "알려주세요.", PRI_COMMON),
        cho_end("bnjsyo",  &[B, N, J, S, O],      "보내주세요.", PRI_COMMON),

        // ── 격식 명령 (~십시오.) ──
        cho_end("sso",     &[S, S, O],            "십시오.",    PRI_POLITE),
        cho_end("hsso",    &[H, S, S, O],         "하십시오.",  PRI_POLITE),
    ]);

    // ── 접속사 (Literal + ChoSeq 병행 등록) ───────────────────────
    v.extend(conj("그러나",   &[G, R, N]));
    v.extend(conj("그러면",   &[G, R, M]));
    v.extend(conj("그러므로", &[G, R, M, R]));
    v.extend(conj("그런데",   &[G, R, D]));
    v.extend(conj("그리고",   &[G, R, G]));
    v.extend(conj("그리하여", &[G, R, H, O]));
    v.extend(conj("그래서",   &[G, R, S]));
    v.extend(conj("그래도",   &[G, R, D]));
    v.extend(conj("그렇지만", &[G, R, J, M]));
    v.extend(conj("하지만",   &[H, J, M]));
    v.extend(conj("따라서",   &[DD, R, S]));
    v.extend(conj("또한",     &[DD, H]));
    v.extend(conj("또는",     &[DD, N]));
    v.extend(conj("혹은",     &[H, O]));
    v.extend(conj("아니면",   &[O, N, M]));
    v.extend(conj("더욱이",   &[D, O, O]));
    v.extend(conj("반면에",   &[B, M, O]));
    v.extend(conj("예컨대",   &[O, K, D]));
    v.extend(conj("예를 들어", &[O, R, D, O]));

    // ── 이메일/업무 템플릿 (Literal) ─────────────────────────────
    v.push(lit("mail-end", "메일끝",
               "감사합니다.\n\n$USERNAME 드림.", Space));
    v.push(lit("btw",  "btw",  "그런데",      Space));
    v.push(lit("imo",  "imo",  "제 생각에는", Space));
    v.push(lit("todo", "tddo", "// TODO: ",  Space));

    // ── 자주 쓰는 낱말 (Literal + ChoSeq) ─────────────────────────
    // ko.wiktionary "자주 쓰이는 한국어 낱말 5800" 상위 빈도 기반.
    // 2음절 이상 일상어·추상명사. PRI_WORD(70) 로 인사말·어미(100)
    // 보다 한 단계 낮은 자리에 둔다. 초성 약어가 기존 항목과 겹칠
    // 때도 제안 목록에 함께 표시되어 사용자가 선택할 수 있다.
    for w in [
        // ── 사람·관계 ──
        "사람", "친구", "가족", "부모", "자녀", "형제", "자매",
        "어른", "아이", "학생", "선생", "선생님", "의사", "기자",
        // ── 시간 ──
        "시간", "오늘", "어제", "내일", "지금", "아침", "점심", "저녁",
        "주말", "평일", "시작", "마지막", "처음", "나중", "요즘", "최근",
        "올해", "내년", "작년",
        // ── 장소·기관 ──
        "학교", "회사", "국가", "정부", "사무실", "공원", "도서관",
        "병원", "은행", "식당", "시장", "가게", "카페", "호텔",
        // ── 학업·업무 ──
        "수업", "공부", "시험", "숙제", "대학", "전공", "연구", "조사",
        "분석", "업무", "회의", "보고", "계획", "결정", "결과", "과정",
        "자료", "정보", "내용", "의미",
        // ── 생활·활동 ──
        "생활", "여행", "운동", "영화", "음악", "음식", "식사", "쇼핑",
        "취미", "독서",
        // ── 추상 개념 ──
        "문제", "방법", "방식", "상황", "경우", "부분", "전체",
        "관계", "이유", "원인", "차이", "변화", "발전", "성장",
        "성공", "실패", "노력", "기회", "해결", "질문", "대답",
        "가치", "목적", "필요", "중요",
        // ── 자연·환경 ──
        "자연", "환경", "날씨", "여름", "가을", "겨울", "계절",
        "하늘", "바다",
        // ── 감정·목표 ──
        "사랑", "행복", "희망", "목표", "미래", "과거", "현재",
        // ── 사회·문화 ──
        "사회", "세계", "나라", "지역", "문화", "역사", "정치", "경제",
        "과학", "기술", "예술", "교육", "철학",
    ] {
        v.extend(word_entry(w));
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_dict_has_sections() {
        let d = starter_dict();
        assert!(d.iter().any(|a| a.id == "gs"));
        assert!(d.iter().any(|a| a.id == "end:습니다"));
        assert!(d.iter().any(|a| a.id == "conj-lit:그러나"));
        assert!(d.iter().any(|a| a.id == "conj-cho:그러나"));
        assert!(d.iter().any(|a| a.id == "mail-end"));
    }

    #[test]
    fn declarative_endings_end_with_period() {
        let d = starter_dict();
        for a in &d {
            if let Trigger::Ending(text) = &a.trigger {
                if text.ends_with("습니다")
                    || text.ends_with("입니다")
                    || text == "합니다"
                    || text == "됩니다"
                    || text == "있습니다"
                    || text == "없습니다"
                    || text == "드립니다"
                    || text == "겠습니다"
                    || text == "드려요"
                    || text == "주세요"
                    || text == "세요"
                    || text == "해요"
                    || text == "네요"
                    || text == "군요"
                {
                    assert!(
                        a.body.ends_with('.'),
                        "declarative ending '{text}' body '{}' lacks a period",
                        a.body
                    );
                }
            }
        }
    }

    #[test]
    fn greetings_end_with_period() {
        let d = starter_dict();
        for id in ["gs", "onhsy", "sg", "jshnd", "chk", "ok", "gd", "hy", "jh", "gt"] {
            let a = d.iter().find(|a| a.id == id).unwrap();
            assert!(a.body.ends_with('.'), "greeting {id} body '{}' lacks a period", a.body);
        }
    }

    #[test]
    fn conjunctions_have_both_triggers() {
        let d = starter_dict();
        for word in ["그러나", "그리고", "하지만", "따라서", "또한", "혹은"] {
            let lit_id = format!("conj-lit:{word}");
            let cho_id = format!("conj-cho:{word}");
            assert!(d.iter().any(|a| a.id == lit_id), "missing {lit_id}");
            assert!(d.iter().any(|a| a.id == cho_id), "missing {cho_id}");
        }
    }

    #[test]
    fn chos_of_extracts_choseong() {
        // 사람 → ㅅ(0x1109) ㄹ(0x1105)
        assert_eq!(chos_of("사람"), vec![0x1109, 0x1105]);
        // 마지막 → ㅁ(0x1106) ㅈ(0x110C) ㅁ(0x1106)
        assert_eq!(chos_of("마지막"), vec![0x1106, 0x110C, 0x1106]);
        // 비한글 스킵
        assert_eq!(chos_of("abc"), Vec::<u32>::new());
        assert_eq!(chos_of("가 나"), vec![0x1100, 0x1102]);
    }

    #[test]
    fn common_words_registered_both_ways() {
        let d = starter_dict();
        for w in ["사람", "시간", "학교", "문제", "희망"] {
            let lit_id = format!("word-lit:{w}");
            let cho_id = format!("word-cho:{w}");
            assert!(d.iter().any(|a| a.id == lit_id), "missing {lit_id}");
            assert!(d.iter().any(|a| a.id == cho_id), "missing {cho_id}");
        }
    }

    #[test]
    fn cho_endings_registered() {
        let d = starter_dict();
        // 핵심 어미들이 cho_seq alias 로 등록되어 있어야 한다.
        for suffix in ["snd", "ond", "hnd", "dnd", "drnd", "gsnd",
                       "isnd", "ogsnd", "sng", "ong", "hng",
                       "jsyo", "hjsyo", "sso"] {
            let id = format!("end-cho-{suffix}");
            let abbr = d.iter().find(|a| a.id == id);
            assert!(abbr.is_some(), "missing cho-ending {id}");
            let a = abbr.unwrap();
            assert!(matches!(a.trigger, Trigger::ChoSeq(_)),
                    "{id} must be cho_seq");
            assert_eq!(a.trigger_on, TriggerEvent::Explicit,
                       "{id} must fire on Tab");
            assert!(a.body.ends_with('.') || a.body.ends_with('?'),
                    "{id} body '{}' must end with terminator", a.body);
        }
    }

    #[test]
    fn cho_endings_question_priority_below_declarative() {
        let d = starter_dict();
        let snd = d.iter().find(|a| a.id == "end-cho-snd").unwrap();
        let sng = d.iter().find(|a| a.id == "end-cho-sng").unwrap();
        assert!(sng.priority < snd.priority,
                "interrogative ㅅㄴㄲ must rank below declarative ㅅㄴㄷ");
    }

    #[test]
    fn common_words_priority_below_greetings() {
        let d = starter_dict();
        let gs = d.iter().find(|a| a.id == "gs").unwrap();
        let saram = d.iter().find(|a| a.id == "word-cho:사람").unwrap();
        assert!(saram.priority < gs.priority,
                "common words must rank below greetings");
    }

    #[test]
    fn all_cho_triggers_use_valid_codepoints() {
        let d = starter_dict();
        for a in &d {
            if let Trigger::ChoSeq(cs) = &a.trigger {
                for &cp in cs {
                    assert!(
                        (0x1100..=0x1112).contains(&cp),
                        "abbr '{}' has out-of-range Cho U+{:04X}",
                        a.id,
                        cp
                    );
                }
            }
        }
    }

    #[test]
    fn endings_do_not_auto_fire_on_space() {
        let d = starter_dict();
        for a in &d {
            if matches!(a.trigger, Trigger::Ending(_)) {
                assert_eq!(a.trigger_on, TriggerEvent::Explicit, "{} auto-fires", a.id);
            }
        }
    }
}
