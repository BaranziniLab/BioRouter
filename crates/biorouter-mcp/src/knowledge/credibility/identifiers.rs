use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Identifiers {
    pub doi: Option<String>,
    pub arxiv: Option<String>,
    pub pmid: Option<String>,
    pub isbn: Option<String>,
}

static DOI_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b10\.\d{4,9}/[-._;()/:A-Z0-9]+\b").unwrap()
});
static ARXIV_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\barXiv:(\d{4}\.\d{4,5})\b|arxiv\.org/(?:abs|pdf)/(\d{4}\.\d{4,5})").unwrap()
});
static PMID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bPMID[: ](\d{6,9})\b|pubmed\.ncbi\.nlm\.nih\.gov/(\d{6,9})").unwrap()
});
static ISBN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bISBN(?:-1[03])?:?\s*(97[89][- ]?(?:\d[- ]?){9}\d|(?:\d[- ]?){9}[\dX])\b").unwrap()
});

pub fn extract(text: &str) -> Identifiers {
    let upper = text.to_ascii_uppercase();
    Identifiers {
        doi: DOI_RE.find(&upper).map(|m| m.as_str().to_lowercase()),
        arxiv: ARXIV_RE
            .captures(text)
            .and_then(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string())),
        pmid: PMID_RE
            .captures(text)
            .and_then(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string())),
        isbn: ISBN_RE
            .captures(text)
            .and_then(|c| c.get(1).map(|m| m.as_str().replace([' ', '-'], ""))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_doi() {
        let id = extract("Reference: 10.1038/s41588-024-01893-6 in the paper.");
        assert_eq!(id.doi.as_deref(), Some("10.1038/s41588-024-01893-6"));
    }

    #[test]
    fn finds_arxiv_url() {
        let id = extract("https://arxiv.org/abs/2403.12345");
        assert_eq!(id.arxiv.as_deref(), Some("2403.12345"));
    }

    #[test]
    fn finds_arxiv_inline() {
        let id = extract("see arXiv:2403.12345 for details");
        assert_eq!(id.arxiv.as_deref(), Some("2403.12345"));
    }

    #[test]
    fn finds_pmid() {
        let id = extract("https://pubmed.ncbi.nlm.nih.gov/12345678");
        assert_eq!(id.pmid.as_deref(), Some("12345678"));
    }

    #[test]
    fn finds_isbn13() {
        let id = extract("ISBN: 978-0-262-04574-1");
        assert_eq!(id.isbn.as_deref(), Some("9780262045741"));
    }

    #[test]
    fn empty_input() {
        assert_eq!(extract(""), Identifiers::default());
    }
}
