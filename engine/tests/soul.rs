//! Sunday's SOUL.md block must load cleanly in Hermes, which scans SOUL.md for
//! prompt injection (tools/threat_patterns.py, "all" and "context" scopes)
//! before putting it in the system prompt.

use openagora::sunday::SOUL_BLOCK;
use regex::RegexBuilder;

const FILLER: &str = r"(?:\w+\s+){0,8}";

#[test]
fn sundays_soul_passes_hermess_injection_scan() {
    // Verbatim from Hermes's _PATTERNS, "all" and "context" scopes.
    let patterns = [
        format!(r"ignore\s+{FILLER}(previous|all|above|prior)\s+{FILLER}instructions"),
        r"system\s+prompt\s+override".into(),
        format!(r"disregard\s+{FILLER}(your|all|any)\s+{FILLER}(instructions|rules|guidelines)"),
        format!(
            r"act\s+as\s+(if|though)\s+{FILLER}you\s+{FILLER}(have\s+no|don't\s+have)\s+{FILLER}(restrictions|limits|rules)"
        ),
        r"<!--[^>]{0,512}(?:ignore|override|system|secret|hidden)[^>]{0,512}-->".into(),
        format!(r"do\s+not\s+{FILLER}tell\s+{FILLER}the\s+user"),
        format!(r"you\s+are\s+{FILLER}now\s+(?:a|an|the)\s+"),
        format!(r"pretend\s+{FILLER}(you\s+are|to\s+be)\s+"),
        format!(r"output\s+{FILLER}(system|initial)\s+prompt"),
        format!(
            r"(respond|answer|reply)\s+without\s+{FILLER}(restrictions|limitations|filters|safety)"
        ),
        format!(r"you\s+have\s+been\s+{FILLER}(updated|upgraded|patched)\s+to"),
        r"\bname\s+yourself\s+\w+".into(),
        r"register\s+(as\s+)?a?\s*node".into(),
        r"(heartbeat|beacon|check[\s\-]?in)\s+(to|with)\s+".into(),
        r"pull\s+(down\s+)?(?:new\s+)?task(?:ing|s)?\b".into(),
        r"connect\s+to\s+the\s+network\b".into(),
        r"you\s+must\s+(?:\w+\s+){0,3}(register|connect|report|beacon)\b".into(),
        r"only\s+use\s+one[\s\-]?liners?\b".into(),
        format!(r"never\s+{FILLER}(?:create|write)\s+{FILLER}(?:script|file)\s+{FILLER}disk"),
        r"\b(?:cobalt\s*strike|sliver|havoc|mythic|metasploit|brainworm)\b".into(),
        r"\bc2\s+(?:server|channel|infrastructure|beacon)\b".into(),
        r"\bcommand\s+and\s+control\b".into(),
    ];
    for pattern in &patterns {
        let re = RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(
            !re.is_match(SOUL_BLOCK),
            "Sunday's SOUL.md matches Hermes's injection pattern {pattern}"
        );
    }
}
