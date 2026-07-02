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

// NANP phone numbers in common written formats (optional +1, separators).
static PHONE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]\d{3}[-.\s]\d{4}\b").unwrap());

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
        for m in PHONE.find_iter(text) {
            push_full(&mut raw, PiiKind::Phone, m);
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
        for s in ["call (415) 555-0132", "415-555-0132", "+1 415.555.0132"] {
            assert!(kinds(s).contains(&PiiKind::Phone), "missed phone in {s:?}");
        }
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
