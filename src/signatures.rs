use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offsets {
    pub r: usize,
    pub g: usize,
    pub g_dark: usize,
    pub b: usize,
    pub b_dark: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    pub name: String,
    pub kind: String,
    pub priority: i32,
    pub pattern: String,
    #[serde(default)]
    pub requires_unique: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureFile {
    pub schema: u32,
    #[serde(default)]
    pub known_build: Option<String>,
    pub offsets: Offsets,
    pub patterns: Vec<Pattern>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub name: String,
    pub kind: String,
    pub priority: i32,
    pub match_count: usize,
    pub start: Option<usize>,
    pub current_color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PatternBytes {
    pub bytes: Vec<u8>,
    pub wildcard: Vec<bool>,
}

pub fn load_signatures(path: &Path) -> anyhow::Result<SignatureFile> {
    let text = fs::read_to_string(path)?;
    let sig: SignatureFile = serde_json::from_str(&text)?;
    if sig.schema != 1 {
        anyhow::bail!("unsupported signatures schema");
    }
    Ok(sig)
}

pub fn parse_pattern(text: &str) -> anyhow::Result<PatternBytes> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        anyhow::bail!("empty signature pattern");
    }
    let mut bytes = Vec::with_capacity(tokens.len());
    let mut wildcard = Vec::with_capacity(tokens.len());
    for token in tokens {
        if token == "??" {
            bytes.push(0);
            wildcard.push(true);
        } else {
            if token.len() != 2 {
                anyhow::bail!("invalid signature byte: {token}");
            }
            bytes.push(u8::from_str_radix(token, 16)?);
            wildcard.push(false);
        }
    }
    Ok(PatternBytes { bytes, wildcard })
}

pub fn find_pattern(data: &[u8], pattern: &PatternBytes) -> Vec<usize> {
    let mut hits = Vec::new();
    let n = pattern.bytes.len();
    if n == 0 || data.len() < n {
        return hits;
    }
    for i in 0..=data.len() - n {
        let mut ok = true;
        for j in 0..n {
            if !pattern.wildcard[j] && data[i + j] != pattern.bytes[j] {
                ok = false;
                break;
            }
        }
        if ok {
            hits.push(i);
        }
    }
    hits
}

pub fn decode_color(data: &[u8], start: usize, offsets: &Offsets) -> anyhow::Result<String> {
    let r_off = start
        .checked_add(offsets.r)
        .ok_or_else(|| anyhow::anyhow!("R offset overflow"))?;
    let g_off = start
        .checked_add(offsets.g)
        .ok_or_else(|| anyhow::anyhow!("G offset overflow"))?;
    let b_off = start
        .checked_add(offsets.b)
        .ok_or_else(|| anyhow::anyhow!("B offset overflow"))?;
    if r_off + 4 > data.len() || g_off >= data.len() || b_off >= data.len() {
        anyhow::bail!("color offset outside renderer");
    }
    let r_bits = u32::from_le_bytes([
        data[r_off],
        data[r_off + 1],
        data[r_off + 2],
        data[r_off + 3],
    ]);
    let r = f32::from_bits(r_bits);
    let rr = (r * 255.0).round().clamp(0.0, 255.0) as u8;
    Ok(format!("#{rr:02X}{:02X}{:02X}", data[g_off], data[b_off]))
}

pub fn scan(data: &[u8], sig: &SignatureFile) -> anyhow::Result<Vec<ScanResult>> {
    let _ = &sig.known_build;
    let mut results = Vec::new();
    let mut patterns = sig.patterns.clone();
    patterns.sort_by_key(|p| p.priority);
    for p in patterns {
        let _ = p.requires_unique;
        let parsed = parse_pattern(&p.pattern)?;
        let hits = find_pattern(data, &parsed);
        let start = if hits.len() == 1 { Some(hits[0]) } else { None };
        let color = match start {
            Some(s) => decode_color(data, s, &sig.offsets).ok(),
            None => None,
        };
        results.push(ScanResult {
            name: p.name,
            kind: p.kind,
            priority: p.priority,
            match_count: hits.len(),
            start,
            current_color: color,
        });
    }
    Ok(results)
}

pub fn choose_unique(results: &[ScanResult]) -> Option<&ScanResult> {
    // Sort order already comes from caller's scan result order (priority).
    results
        .iter()
        .find(|r| r.match_count == 1 && r.start.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_pattern_matches_unique_bytes() {
        let data = [0x00, 0xAA, 0xBB, 0xCC, 0x11, 0x22];
        let pattern = parse_pattern("AA ?? CC").unwrap();
        assert_eq!(find_pattern(&data, &pattern), vec![1]);
    }

    #[test]
    fn patterns_can_be_rejected_when_ambiguous() {
        let data = [0xAA, 0x01, 0xAA, 0x02];
        let pattern = parse_pattern("AA ??").unwrap();
        assert_eq!(find_pattern(&data, &pattern).len(), 2);
    }

    #[test]
    fn decodes_float_red_and_byte_green_blue() {
        let mut data = vec![0u8; 64];
        let offsets = Offsets {
            r: 4,
            g: 8,
            g_dark: 9,
            b: 10,
            b_dark: 11,
        };
        data[4..8].copy_from_slice(&(1.0f32).to_le_bytes());
        data[8] = 0x00;
        data[10] = 0x7A;
        assert_eq!(decode_color(&data, 0, &offsets).unwrap(), "#FF007A");
    }
}
