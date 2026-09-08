use std::{fs, path::Path};

use crate::{process::CheckRecord, time::now_utc, verify::VerifyMode};

pub(crate) struct ArtifactRecord {
    pub(crate) file: String,
    pub(crate) sha256: String,
}

pub(crate) fn write_validation(
    root: &Path,
    mode: VerifyMode,
    started: &str,
    failure: Option<&String>,
    checks: &[CheckRecord],
    artifacts: &[ArtifactRecord],
) -> Result<(), String> {
    let path = root.join(".ai/VALIDATION.json");
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"schema_version\": 1,\n");
    json.push_str(&format!("  \"started_utc\": {},\n", json_string(started)));
    json.push_str(&format!(
        "  \"finished_utc\": {},\n",
        json_string(&now_utc())
    ));
    json.push_str(&format!("  \"mode\": {},\n", json_string(mode.as_str())));
    json.push_str(&format!("  \"passed\": {},\n", failure.is_none()));
    match failure {
        Some(error) => json.push_str(&format!("  \"failure\": {},\n", json_string(error))),
        None => json.push_str("  \"failure\": null,\n"),
    }
    json.push_str("  \"scope\": \"Debug native Vulkan wgpu execution of Slang-generated WGSL; SPIR-V export validation is separate.\",\n");
    json.push_str("  \"untested\": [\"browser WebGPU\", \"Metal\", \"DXIL\", \"other GPU vendors\", \"declared minimum Rust version\"],\n");
    json.push_str("  \"checks\": [\n");
    for (index, check) in checks.iter().enumerate() {
        if index != 0 {
            json.push_str(",\n");
        }
        json.push_str("    {\n");
        json.push_str(&format!(
            "      \"program\": {},\n",
            json_string(&check.program)
        ));
        json.push_str("      \"arguments\": [");
        for (arg_index, arg) in check.arguments.iter().enumerate() {
            if arg_index != 0 {
                json.push_str(", ");
            }
            json.push_str(&json_string(arg));
        }
        json.push_str("],\n");
        match check.exit_code {
            Some(code) => json.push_str(&format!("      \"exit_code\": {code},\n")),
            None => json.push_str("      \"exit_code\": null,\n"),
        }
        json.push_str(&format!(
            "      \"stdout\": {},\n",
            json_string(&check.stdout)
        ));
        json.push_str(&format!(
            "      \"stderr\": {}\n",
            json_string(&check.stderr)
        ));
        json.push_str("    }");
    }
    json.push_str("\n  ],\n");
    json.push_str("  \"spirv_artifacts\": [\n");
    for (index, artifact) in artifacts.iter().enumerate() {
        if index != 0 {
            json.push_str(",\n");
        }
        json.push_str("    {\n");
        json.push_str(&format!(
            "      \"file\": {},\n",
            json_string(&artifact.file)
        ));
        json.push_str(&format!(
            "      \"sha256\": {},\n",
            json_string(&artifact.sha256)
        ));
        json.push_str("      \"validation_target\": \"vulkan1.2\",\n");
        json.push_str("      \"status\": \"passed\"\n");
        json.push_str("    }");
    }
    json.push_str("\n  ]\n}\n");
    fs::write(path, json).map_err(|error| error.to_string())
}

pub(crate) fn compact_validation_summary(text: &str) -> String {
    let field = |name: &str| -> Option<String> {
        let pattern = format!("\"{name}\":");
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with(&pattern))?;
        Some(
            line.split_once(':')?
                .1
                .trim()
                .trim_end_matches(',')
                .trim_matches('"')
                .to_owned(),
        )
    };
    let finished = field("finished_utc").unwrap_or_else(|| "unknown".to_owned());
    let mode = field("mode").unwrap_or_else(|| "unknown".to_owned());
    let passed = field("passed").unwrap_or_else(|| "unknown".to_owned());
    format!("finished={finished} mode={mode} passed={passed}")
}

pub(crate) fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha256(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut data = input.to_vec();
    let bit_len = (data.len() as u64) * 8;
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    let mut h = H0;
    let (chunks, remainder) = data.as_chunks::<64>();
    debug_assert!(remainder.is_empty());
    for chunk in chunks {
        let mut w = [0u32; 64];
        let (words, remainder) = chunk.as_chunks::<4>();
        debug_assert!(remainder.is_empty());
        for (i, word) in words.iter().take(16).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}
