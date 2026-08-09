//! Local PII / PHI detector — regex + checksum based, **no network, no model**.
//!
//! Tuned for precision on the identifiers that matter for biomedical/clinical
//! text (the BRSDK's primary use): SSN, MRN, dates of birth, phone, email,
//! credit cards (Luhn-validated), and IP addresses, plus a conservative,
//! keyword-anchored person-name pass. The high-noise types (names, addresses)
//! are deliberately conservative so the detector defaults toward *masking real
//! identifiers* rather than flooding ordinary prose with false positives.
//!
//! This is the on-device baseline. The single seam [`PiiDetector::scan`] is
//! where a Presidio-style analyzer or an ONNX NER could later be merged in
//! without touching callers.

use once_cell::sync::Lazy;
use regex::Regex;

/// A category of detected personal / protected information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PiiKind {
    Ssn,
    Mrn,
    Dob,
    Phone,
    Email,
    CreditCard,
    IpAddress,
    PersonName,
}

impl PiiKind {
    /// Short uppercase tag used in the `[REDACTED:…]` replacement.
    pub fn tag(self) -> &'static str {
        match self {
            PiiKind::Ssn => "SSN",
            PiiKind::Mrn => "MRN",
            PiiKind::Dob => "DOB",
            PiiKind::Phone => "PHONE",
            PiiKind::Email => "EMAIL",
            PiiKind::CreditCard => "CC",
            PiiKind::IpAddress => "IP",
            PiiKind::PersonName => "NAME",
        }
    }
}

/// A single detected span (byte offsets into the scanned text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiiMatch {
    pub kind: PiiKind,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

// ── compiled patterns (built once) ──

static EMAIL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b").unwrap());

// SSN only in clearly-formatted form (dashed/spaced) to avoid matching arbitrary
// 9-digit ids. The `regex` crate has no lookahead, so structural validity
// (area not 000/666/900-999, group not 00, serial not 0000) is checked in code
// by `ssn_valid` rather than in the pattern.
static SSN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d{3}[- ]\d{2}[- ]\d{4}\b").unwrap());

/// Reject structurally-invalid SSNs (never-issued area/group/serial values).
fn ssn_valid(s: &str) -> bool {
    let digits: Vec<u8> = s
        .bytes()
        .filter(|b| b.is_ascii_digit())
        .map(|b| b - b'0')
        .collect();
    if digits.len() != 9 {
        return false;
    }
    let area = digits[0] as u16 * 100 + digits[1] as u16 * 10 + digits[2] as u16;
    let group = digits[3] * 10 + digits[4];
    let serial = digits[5..9].iter().fold(0u16, |a, &d| a * 10 + d as u16);
    area != 0 && area != 666 && area < 900 && group != 0 && serial != 0
}

// NANP phone numbers, matched on SHAPE rather than on "ten digits somewhere".
//
// ⚠ The separator class must never contain `\s`. It used to be `[-.\s]`, and
// `\s` matches a NEWLINE — so a tool that printed the integers 1..1500 one per
// line handed the model `998\n999\n1000`, a textbook `\d{3} sep \d{3} sep
// \d{4}`, and the whole result was framed as containing PHI. Any numeric tool
// output trips that: line counts, ids, measurements, a CSV column. The cost is
// not the noise — it is that a guardrail which cries wolf on a column of
// integers is one the model learns to discount on a real phone number.
//
// The candidate below is deliberately permissive (every separator optional) and
// [`phone_shape_ok`] does the deciding, the same split SSN/[`ssn_valid`] and
// credit cards/[`luhn_ok`] already use in this file.
const PHONE_SHAPE: &str = r"(?:\+\d{1,3}[-. ]?)?\(?\d{3}\)?[-. ]?\d{3}[-. ]?\d{4}";

static PHONE_CANDIDATE: Lazy<Regex> = Lazy::new(|| Regex::new(PHONE_SHAPE).unwrap());

// A LABEL is itself a phone signal, and it licenses the forms the shape rule
// alone rejects: `Phone: 415 555 0132`, `Tel 4155550132`. Built from the same
// `PHONE_SHAPE` so the two patterns cannot drift apart.
//
// "cell" and "mobile" are deliberately NOT labels here: this detector's primary
// text is biomedical, where "cell counts: 300 400 5000" is ordinary prose and
// would become a reported phone number.
static PHONE_LABELED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:tel(?:ephone)?|phone|fax)\b\.?\s{{0,3}}[:#]?\s{{0,3}}({PHONE_SHAPE})"
    ))
    .unwrap()
});

/// Structural plausibility, shared by both phone paths.
///
/// The NANP rules are what most "ten digits in a row" false positives fail:
/// neither the area code nor the exchange may begin with 0 or 1, so `100 200
/// 3000` is not a phone number however it is punctuated. They are applied to
/// the trailing ten digits of a `+CC` number too — only NANP-shaped bodies
/// match `PHONE_SHAPE` at all, so there is no non-NANP number here to mis-judge.
fn phone_digits_plausible(s: &str) -> bool {
    let digits: Vec<u8> = s
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    let has_plus = s.starts_with('+');
    // E.164 caps a number at 15 digits; NANP is 10, or 11 with the country code.
    if digits.len() < 10 || digits.len() > 15 {
        return false;
    }
    if !has_plus && (digits.len() > 11 || (digits.len() == 11 && digits[0] != 1)) {
        return false;
    }
    let national = &digits[digits.len() - 10..];
    national[0] >= 2 && national[3] >= 2
}

/// A candidate is a phone number only if it *looks* like one.
///
/// Beyond plausibility it must carry a phone SIGNAL — a `+` country code, a
/// parenthesised area code, or `-`/`.` group separators. A bare run of digits,
/// or digit groups separated only by spaces, is an accession number, a
/// measurement or a spreadsheet column far more often than it is a phone
/// number; when it really is one, it is normally labelled, and
/// [`PHONE_LABELED`] catches that.
fn phone_shape_ok(s: &str) -> bool {
    phone_digits_plausible(s)
        && (s.starts_with('+')
            || (s.contains('(') && s.contains(')'))
            || s.contains('-')
            || s.contains('.'))
}

/// True when the span is not carved out of the middle of a longer run of digits
/// (an accession number, a timestamp, a genomic coordinate). Replaces the `\b`
/// the pattern cannot use now that it may begin and end with an optional
/// separator. Byte comparison is UTF-8 safe: a continuation byte is never an
/// ASCII digit.
fn not_inside_a_longer_number(text: &str, start: usize, end: usize) -> bool {
    let b = text.as_bytes();
    (start == 0 || !b[start - 1].is_ascii_digit()) && (end >= b.len() || !b[end].is_ascii_digit())
}

// MRN: keyword-anchored (MRNs have no universal format), capturing the id.
static MRN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bMRN\s*[:#]?\s*([A-Z0-9]{5,12})\b").unwrap());

// Date of birth: a date adjacent to a DOB/born keyword, capturing the date.
static DOB: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?:dob|d\.o\.b\.?|date of birth|born)\b\s*[:\-]?\s*\(?(\d{1,2}[/\-]\d{1,2}[/\-]\d{2,4})\)?",
    )
    .unwrap()
});

// Candidate card numbers (13–19 digits, optional space/dash separators);
// validated with Luhn before being reported.
static CC_CANDIDATE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(?:\d[ -]?){13,19}\b").unwrap());

static IPV4: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b").unwrap()
});

// Conservative person-name pass: a title or explicit "name:"/"patient" keyword
// followed by 1–2 Capitalized words. Keyword-anchored to keep precision high.
static PERSON_NAME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:(?i:\b(?:patient|name|mr|mrs|ms|dr|miss)\b\.?[:\s]+))([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2})",
    )
    .unwrap()
});

/// Luhn checksum (credit-card validity).
fn luhn_ok(digits: &str) -> bool {
    let ds: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if ds.len() < 13 || ds.len() > 19 {
        return false;
    }
    let mut sum = 0u32;
    let mut dbl = false;
    for &d in ds.iter().rev() {
        let mut v = d;
        if dbl {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
        dbl = !dbl;
    }
    sum.is_multiple_of(10)
}

/// The local detector. Cheap to construct (patterns are shared statics).
#[derive(Debug, Default, Clone, Copy)]
pub struct PiiDetector;

impl PiiDetector {
    pub fn new() -> Self {
        PiiDetector
    }

    /// Find all PII/PHI spans in `text`. Overlapping matches are resolved in
    /// favor of the earliest, then longest, so the returned spans are sorted and
    /// non-overlapping — safe to feed directly into [`PiiDetector::mask`].
    pub fn scan(&self, text: &str) -> Vec<PiiMatch> {
        let mut raw: Vec<PiiMatch> = Vec::new();

        let push_full = |raw: &mut Vec<PiiMatch>, kind: PiiKind, m: regex::Match| {
            raw.push(PiiMatch {
                kind,
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
            });
        };

        for m in EMAIL.find_iter(text) {
            push_full(&mut raw, PiiKind::Email, m);
        }
        for m in SSN.find_iter(text) {
            if ssn_valid(m.as_str()) {
                push_full(&mut raw, PiiKind::Ssn, m);
            }
        }
        for m in PHONE_CANDIDATE.find_iter(text) {
            if not_inside_a_longer_number(text, m.start(), m.end()) && phone_shape_ok(m.as_str()) {
                push_full(&mut raw, PiiKind::Phone, m);
            }
        }
        // A labelled number needs no shape signal — the label is the signal.
        // Overlap resolution below drops the duplicate when both paths agree.
        for caps in PHONE_LABELED.captures_iter(text) {
            if let Some(g) = caps.get(1) {
                if phone_digits_plausible(g.as_str()) {
                    push_full(&mut raw, PiiKind::Phone, g);
                }
            }
        }
        for m in IPV4.find_iter(text) {
            push_full(&mut raw, PiiKind::IpAddress, m);
        }
        // Keyword-anchored kinds: redact the captured identifier, not the keyword.
        for caps in MRN.captures_iter(text) {
            if let Some(g) = caps.get(1) {
                push_full(&mut raw, PiiKind::Mrn, g);
            }
        }
        for caps in DOB.captures_iter(text) {
            if let Some(g) = caps.get(1) {
                push_full(&mut raw, PiiKind::Dob, g);
            }
        }
        for caps in PERSON_NAME.captures_iter(text) {
            if let Some(g) = caps.get(1) {
                push_full(&mut raw, PiiKind::PersonName, g);
            }
        }
        // Credit cards: only Luhn-valid candidates.
        for m in CC_CANDIDATE.find_iter(text) {
            if luhn_ok(m.as_str()) {
                push_full(&mut raw, PiiKind::CreditCard, m);
            }
        }

        // Resolve overlaps: earliest start first, then longest span wins.
        raw.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
        let mut out: Vec<PiiMatch> = Vec::new();
        let mut cursor = 0usize;
        for m in raw {
            if m.start >= cursor {
                cursor = m.end;
                out.push(m);
            }
        }
        out
    }

    /// True if `text` contains any PII/PHI.
    pub fn has_pii(&self, text: &str) -> bool {
        !self.scan(text).is_empty()
    }

    /// Return `text` with every detected span replaced by `[REDACTED:KIND]`,
    /// plus the spans that were masked.
    pub fn mask(&self, text: &str) -> (String, Vec<PiiMatch>) {
        let matches = self.scan(text);
        if matches.is_empty() {
            return (text.to_string(), matches);
        }
        let mut out = String::with_capacity(text.len());
        let mut last = 0usize;
        for m in &matches {
            out.push_str(text.get(last..m.start).unwrap_or_default());
            out.push_str(&format!("[REDACTED:{}]", m.kind.tag()));
            last = m.end;
        }
        out.push_str(text.get(last..).unwrap_or_default());
        (out, matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<PiiKind> {
        PiiDetector::new()
            .scan(text)
            .into_iter()
            .map(|m| m.kind)
            .collect()
    }

    #[test]
    fn detects_email() {
        assert!(kinds("contact jane.doe@hospital.org for results").contains(&PiiKind::Email));
        // Not an email.
        assert!(!kinds("the dose is 5mg @ noon").contains(&PiiKind::Email));
    }

    #[test]
    fn detects_dashed_ssn_but_not_arbitrary_9_digits() {
        assert!(kinds("SSN 123-45-6789 on file").contains(&PiiKind::Ssn));
        // A bare 9-digit accession number must NOT be flagged as SSN.
        assert!(!kinds("accession 123456789 archived").contains(&PiiKind::Ssn));
        // Invalid area numbers rejected.
        assert!(!kinds("000-12-3456").contains(&PiiKind::Ssn));
        assert!(!kinds("666-12-3456").contains(&PiiKind::Ssn));
    }

    #[test]
    fn detects_phone_formats() {
        for s in [
            "call (415) 555-0132",
            "415-555-0132",
            "+1 415.555.0132",
            "+1 (415) 555-0132",
            "415.555.0132",
            // Space-separated and bare digits have no shape signal of their
            // own, so they are carried by their label — which is how a real
            // phone number in a record is nearly always written.
            "Phone: 415 555 0132",
            "Telephone: (415) 555-0132",
            "Tel 4155550132",
            "Fax: 415-555-0199",
        ] {
            assert!(kinds(s).contains(&PiiKind::Phone), "missed phone in {s:?}");
        }
    }

    /// Measured false positive (2026-08 subagent stress pass): a subagent
    /// returned the integers 1..1500, one per line, and the parent received the
    /// whole thing prefixed with "1 potential PII/PHI span(s) detected (PHONE)".
    /// `998\n999\n1000` was the span — the separator class was `[-.\s]`, and
    /// `\s` matches a newline.
    ///
    /// Every false positive here teaches the model to discount the warning,
    /// which costs more than the miss it is guarding against.
    #[test]
    fn a_run_of_plain_integers_is_not_a_phone_number() {
        let column: String = (1..=1500).map(|n| format!("{n}\n")).collect();
        let hits = PiiDetector::new().scan(&column);
        assert!(
            hits.is_empty(),
            "the integers 1..1500 one per line are not PII: {:?}",
            hits.iter().map(|m| (m.kind, &m.text)).collect::<Vec<_>>()
        );

        // The same digits on ONE line are not a phone number either: groups
        // separated only by spaces carry no phone signal.
        assert!(!kinds("998 999 1000").contains(&PiiKind::Phone));
        assert!(!kinds("500 600 7000").contains(&PiiKind::Phone));
        // Ordinary numeric tool output: ids, counts, coordinates.
        assert!(!kinds("accession 4155550132 archived").contains(&PiiKind::Phone));
        assert!(!kinds("rows: 1024 4096 65536").contains(&PiiKind::Phone));
        assert!(!kinds("chr1:155-550-1320").contains(&PiiKind::Phone));
        // An area code or exchange starting with 0/1 is not dialable, which is
        // what a punctuated id usually looks like.
        assert!(!kinds("100-200-3000").contains(&PiiKind::Phone));
        assert!(!kinds("415-155-0132").contains(&PiiKind::Phone));
        // Too long / too short to be a phone number.
        assert!(!kinds("4155550132999999").contains(&PiiKind::Phone));
        assert!(!kinds("415-555-013").contains(&PiiKind::Phone));
    }

    #[test]
    fn detects_mrn_keyword_anchored_and_redacts_id_only() {
        let d = PiiDetector::new();
        let (masked, matches) = d.mask("Patient MRN: AB12345 admitted");
        assert!(matches.iter().any(|m| m.kind == PiiKind::Mrn));
        // The MRN value is redacted; the literal "MRN:" label stays.
        assert!(masked.contains("MRN:"));
        assert!(masked.contains("[REDACTED:MRN]"));
        assert!(!masked.contains("AB12345"));
    }

    #[test]
    fn detects_dob_only_when_keyword_anchored() {
        assert!(kinds("DOB: 03/14/1981").contains(&PiiKind::Dob));
        assert!(kinds("born 3-14-1981").contains(&PiiKind::Dob));
        // A bare date (e.g. an appointment) is not flagged as DOB.
        assert!(!kinds("follow-up on 03/14/2026").contains(&PiiKind::Dob));
    }

    #[test]
    fn detects_credit_card_only_when_luhn_valid() {
        // 4111 1111 1111 1111 is the canonical Luhn-valid test number.
        assert!(kinds("card 4111 1111 1111 1111").contains(&PiiKind::CreditCard));
        // Same shape, last digit changed → Luhn-invalid → not flagged.
        assert!(!kinds("card 4111 1111 1111 1112").contains(&PiiKind::CreditCard));
    }

    #[test]
    fn detects_ipv4_but_not_version_strings() {
        assert!(kinds("client 192.168.1.42 connected").contains(&PiiKind::IpAddress));
        assert!(!kinds("running v1.86.0 build").contains(&PiiKind::IpAddress));
    }

    #[test]
    fn detects_keyword_anchored_person_name() {
        assert!(kinds("Patient: John Smith presented").contains(&PiiKind::PersonName));
        assert!(kinds("seen by Dr Alice Wong").contains(&PiiKind::PersonName));
        // Capitalized non-name prose is NOT flagged (no name keyword).
        assert!(!kinds("The Mitochondria Powerhouse").contains(&PiiKind::PersonName));
    }

    #[test]
    fn realistic_clinical_note_masks_all_phi_and_keeps_clinical_content() {
        let note = "Patient: John Smith, MRN: A1234567, DOB: 07/22/1968. \
                    Reachable at 415-555-0188 or john.smith@example.com. \
                    Presented with elevated CFTR-related symptoms; started on ivacaftor 150mg BID.";
        let d = PiiDetector::new();
        let (masked, matches) = d.mask(note);

        // Every PHI identifier is gone from the output.
        assert!(!masked.contains("John Smith"));
        assert!(!masked.contains("A1234567"));
        assert!(!masked.contains("07/22/1968"));
        assert!(!masked.contains("415-555-0188"));
        assert!(!masked.contains("john.smith@example.com"));

        // The clinically meaningful content survives untouched.
        assert!(masked.contains("CFTR-related symptoms"));
        assert!(masked.contains("ivacaftor 150mg BID"));

        // We detected at least one of each expected kind.
        let found: std::collections::HashSet<_> = matches.iter().map(|m| m.kind).collect();
        for k in [
            PiiKind::PersonName,
            PiiKind::Mrn,
            PiiKind::Dob,
            PiiKind::Phone,
            PiiKind::Email,
        ] {
            assert!(
                found.contains(&k),
                "expected to detect {k:?} in the clinical note"
            );
        }
    }

    #[test]
    fn clean_text_is_untouched() {
        let d = PiiDetector::new();
        let s = "Differential expression analysis of 2,000 genes showed no significant change.";
        let (masked, matches) = d.mask(s);
        assert_eq!(masked, s);
        assert!(matches.is_empty());
    }

    #[test]
    fn masked_output_has_no_overlapping_corruption() {
        // Adjacent identifiers must each be replaced cleanly with no leftover.
        let d = PiiDetector::new();
        let (masked, _) = d.mask("email a@b.co phone 415-555-0001 done");
        assert!(masked.contains("[REDACTED:EMAIL]"));
        assert!(masked.contains("[REDACTED:PHONE]"));
        assert!(masked.contains("done"));
        assert!(masked.starts_with("email "));
    }
}
