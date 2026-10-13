//! oracle.rs — score a campaign's responses against the design's own
//! detection path.
//!
//! The oracle is deliberately generic: a 4-bit encoding field that silently
//! accepts a non-canonical value, with the design's corresponding field-alert
//! bit left clear, is a violation of the encoding contract. Nothing here knows
//! about any specific defect.

use crate::campaign::{Probe, MUBI4_FALSE, MUBI4_TRUE};
use crate::surface::Surface;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Non-canonical value stored, and the design did not report it.
    UndetectedAccept,
    /// Non-canonical value stored, and the corresponding field alert fired.
    DetectedAccept,
    /// The design refused / sanitised the non-canonical value.
    Sanitised,
    /// Canonical value: not a violation, used as a control.
    Canonical,
    /// Response missing or unparseable.
    Inconclusive,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub probe: usize,
    pub register: String,
    pub field: String,
    pub value: u32,
    pub observed: Option<u32>,
    pub alert_name: Option<String>,
    pub alert_value: Option<u64>,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Copy)]
struct Resp<'a> {
    kind: &'a str,
    addr: u64,
    data: u64,
    err: u64,
    intg: u64,
}

fn parse_resp(line: &str) -> Option<Resp<'_>> {
    let mut it = line.split_whitespace();
    let kind = it.next()?;
    if kind != "W" && kind != "R" {
        return None;
    }
    let addr = u64::from_str_radix(it.next()?.trim_start_matches("0x"), 16).ok()?;
    let data = u64::from_str_radix(it.next()?.trim_start_matches("0x"), 16).ok()?;
    let err = it
        .next()?
        .split('=')
        .nth(1)?
        .parse::<u64>()
        .unwrap_or(0);
    let intg = it
        .next()?
        .split('=')
        .nth(1)?
        .parse::<u64>()
        .unwrap_or(0);
    Some(Resp {
        kind,
        addr,
        data,
        err,
        intg,
    })
}

/// The status bit a design is expected to raise for a given encoding field:
/// conventionally `<FIELD>_FIELD_ALERT`. Returns the bit position.
fn alert_bit_for(s: &Surface, status_reg: &str, field: &str) -> Option<u32> {
    let want = format!("{}_FIELD_ALERT", field.to_ascii_uppercase());
    let reg = s.registers.iter().find(|r| r.name == status_reg)?;
    for f in &reg.fields {
        if f.name.eq_ignore_ascii_case(&want) {
            return f.lsb;
        }
    }
    // Fallback: any alert field whose name contains the field name.
    for f in &reg.fields {
        if f.name.to_ascii_uppercase().contains(&field.to_ascii_uppercase())
            && f.name.to_ascii_uppercase().contains("ALERT")
        {
            return f.lsb;
        }
    }
    None
}

pub fn analyse(s: &Surface, probes: &[Probe], resp_lines: &[String]) -> (Vec<Finding>, String) {
    let responses: Vec<Option<Resp>> = resp_lines.iter().map(|l| parse_resp(l)).collect();

    // A field is only interesting if it is writable at all. Establish that by
    // checking whether the canonical true encoding was stored.
    let mut writable: BTreeMap<(String, String), bool> = BTreeMap::new();
    for p in probes {
        if p.value == MUBI4_TRUE {
            if let Some(Some(r)) = responses.get(p.readback_idx) {
                let nib = (r.data >> p.lsb) & 0xf;
                writable.insert(
                    (p.register.clone(), p.field.clone()),
                    nib as u32 == MUBI4_TRUE,
                );
            }
        }
    }

    let mut findings = Vec::new();
    for p in probes {
        let readback = responses.get(p.readback_idx).and_then(|x| x.as_ref());
        let observed = readback.map(|r| ((r.data >> p.lsb) & 0xf) as u32);

        let mut alert_name = None;
        let mut alert_value = None;
        for (name, idx) in &p.status {
            if let Some(Some(r)) = responses.get(*idx) {
                if let Some(bit) = alert_bit_for(s, name, &p.field) {
                    alert_name = Some(format!("{name}[{bit}]"));
                    alert_value = Some((r.data >> bit) & 1);
                    break;
                }
            }
        }
        let alert = alert_value.unwrap_or(0);

        let verdict = if p.value == MUBI4_TRUE || p.value == MUBI4_FALSE {
            Verdict::Canonical
        } else if observed.is_none() {
            Verdict::Inconclusive
        } else if observed != Some(p.value) {
            Verdict::Sanitised
        } else if alert == 1 {
            Verdict::DetectedAccept
        } else {
            Verdict::UndetectedAccept
        };

        findings.push(Finding {
            probe: p.probe,
            register: p.register.clone(),
            field: p.field.clone(),
            value: p.value,
            observed,
            alert_name,
            alert_value,
            verdict,
        });
    }

    let report = render(s, &findings, &writable);
    (findings, report)
}

fn render(
    s: &Surface,
    findings: &[Finding],
    writable: &BTreeMap<(String, String), bool>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Encoding-contract sweep — {}\n\n", s.ip));
    let undetected = findings
        .iter()
        .filter(|f| f.verdict == Verdict::UndetectedAccept)
        .count();
    let detected = findings
        .iter()
        .filter(|f| f.verdict == Verdict::DetectedAccept)
        .count();
    let sanitised = findings
        .iter()
        .filter(|f| f.verdict == Verdict::Sanitised)
        .count();
    out.push_str(&format!(
        "{} probes: {} undetected non-canonical acceptances, {} detected, {} sanitised.\n\n",
        findings.len(),
        undetected,
        detected,
        sanitised
    ));

    // Group by field so the report reads per control.
    let mut by_field: BTreeMap<(String, String), Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        by_field
            .entry((f.register.clone(), f.field.clone()))
            .or_default()
            .push(f);
    }

    out.push_str("| register | field | writable | undetected values | alert bit | verdict |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for ((reg, field), fs) in &by_field {
        let bad: Vec<String> = fs
            .iter()
            .filter(|f| f.verdict == Verdict::UndetectedAccept)
            .map(|f| format!("0x{:x}", f.value))
            .collect();
        let w = writable
            .get(&(reg.clone(), field.clone()))
            .map(|b| if *b { "yes" } else { "no" })
            .unwrap_or("?");
        let alert = fs
            .iter()
            .find_map(|f| f.alert_name.clone())
            .unwrap_or_else(|| "-".into());
        let verdict = if !bad.is_empty() {
            "UNDETECTED ACCEPT"
        } else if fs.iter().any(|f| f.verdict == Verdict::DetectedAccept) {
            "detected"
        } else {
            "sanitised/none"
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            reg,
            field,
            w,
            if bad.is_empty() {
                "-".to_string()
            } else {
                bad.join(" ")
            },
            alert,
            verdict
        ));
    }

    out.push_str("\n## Undetected acceptances (detail)\n\n");
    let mut any = false;
    for f in findings
        .iter()
        .filter(|f| f.verdict == Verdict::UndetectedAccept)
    {
        any = true;
        out.push_str(&format!(
            "- probe {}: wrote non-canonical 0x{:x} to {}.{}; read back 0x{:x}; {} = {} (design reported nothing)\n",
            f.probe,
            f.value,
            f.register,
            f.field,
            f.observed.unwrap_or(0),
            f.alert_name.clone().unwrap_or_else(|| "status".into()),
            f.alert_value.unwrap_or(0),
        ));
    }
    if !any {
        out.push_str("None: every non-canonical value was either sanitised or reported.\n");
    }
    out
}
