use once_cell::sync::Lazy;
use std::collections::HashSet;

/// Curated set of publisher names recognised as peer-reviewed-paper-grade.
/// Names are matched case-insensitively against the `publisher` field returned
/// by Crossref or OpenAlex. The list is intentionally generous; users can add
/// project-specific entries via credibility.yaml.
static ALLOWLIST: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    HashSet::from([
        // Big Five STM
        "elsevier",
        "elsevier bv",
        "elsevier ltd",
        "springer nature",
        "springer",
        "nature publishing group",
        "nature portfolio",
        "wiley",
        "john wiley & sons",
        "wiley-blackwell",
        "taylor & francis",
        "taylor and francis",
        "routledge",
        "informa uk limited",
        "sage publications",
        "sage publications, ltd.",
        // University presses
        "oxford university press",
        "cambridge university press",
        "harvard university press",
        "princeton university press",
        "yale university press",
        "mit press",
        "stanford university press",
        "university of chicago press",
        "columbia university press",
        "cornell university press",
        "johns hopkins university press",
        "university of california press",
        "duke university press",
        "edinburgh university press",
        "manchester university press",
        "university of toronto press",
        // Scholarly societies
        "ieee",
        "acs publications",
        "american chemical society",
        "association for computing machinery",
        "acm",
        "american physical society",
        "aps",
        "american institute of physics",
        "aip publishing",
        "royal society of chemistry",
        "rsc",
        "american association for the advancement of science",
        "aaas",
        "american psychological association",
        "american mathematical society",
        "iop publishing",
        "the royal society",
        "royal society publishing",
        "bmj publishing group",
        "bmj",
        "annual reviews",
        // Open-access
        "plos",
        "public library of science",
        "frontiers media s.a.",
        "frontiers",
        "mdpi",
        "hindawi",
        // Medical / scientific
        "wolters kluwer",
        "wolters kluwer health",
        "lippincott williams & wilkins",
        "karger",
        "thieme",
        "mary ann liebert",
        "world scientific",
        "bentham science",
        "de gruyter",
        "brill",
        "emerald",
        "edp sciences",
        // Other notable
        "cell press",
        "the lancet",
        "lancet",
        "massachusetts medical society", // publisher of NEJM
        "jama network",
    ])
});

pub fn is_peer_reviewed_publisher(publisher: &str) -> bool {
    let normal = publisher.to_lowercase();
    if ALLOWLIST.contains(normal.as_str()) {
        return true;
    }
    // Substring match on a small set of distinctive tokens to catch slight variants.
    for needle in [
        "elsevier",
        "springer",
        "wiley",
        "ieee",
        "oxford university",
        "cambridge university",
    ] {
        if normal.contains(needle) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_publishers_match() {
        for p in [
            "Elsevier",
            "Springer Nature",
            "Wiley",
            "IEEE",
            "Oxford University Press",
            "Massachusetts Medical Society",
            "Frontiers Media S.A.",
            "PLOS",
        ] {
            assert!(is_peer_reviewed_publisher(p), "should match {p}");
        }
    }

    #[test]
    fn variant_strings_match_via_substring() {
        assert!(is_peer_reviewed_publisher("Elsevier Inc."));
        assert!(is_peer_reviewed_publisher(
            "Springer International Publishing"
        ));
    }

    #[test]
    fn unknown_publishers_dont_match() {
        assert!(!is_peer_reviewed_publisher("Some Random Blog"));
        assert!(!is_peer_reviewed_publisher("Medium"));
    }
}
