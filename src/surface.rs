//! surface.rs — autonomous attack-surface discovery.
//!
//! Sources (both in-tree, both machine-readable, no hand-written address lists):
//!   1. `hw/ip/<ip>/data/<ip>.hjson`            — register/field semantics
//!   2. `hw/ip/<ip>/rtl/<ip>_reg_pkg.sv`        — authoritative register offsets
//!
//! We deliberately do NOT recompute offsets from field widths (regtool is the
//! authority); we read them from the generated register package.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub bits: String,
    /// msb-first bit indices parsed from `bits`, when parseable.
    pub msb: Option<u32>,
    pub lsb: Option<u32>,
    pub resval: Option<u64>,
    pub swaccess: Option<String>,
    pub desc: Option<String>,
    /// Field whose semantics are a multi-bit-boolean / locking encoding.
    pub is_mubi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Register {
    pub name: String,
    /// Byte offset parsed from the generated reg_pkg. None if unavailable.
    pub offset: Option<u64>,
    pub swaccess: Option<String>,
    pub hwaccess: Option<String>,
    /// Register whose value gates writes to this register (`regwen`).
    pub regwen: Option<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surface {
    pub ip: String,
    pub tree: String,
    pub hjson: Option<String>,
    pub reg_pkg: Option<String>,
    pub registers: Vec<Register>,
    /// Names of registers that act as write-enable locks for other registers.
    pub lock_registers: Vec<String>,
}

impl Surface {
    pub fn reg_by_name(&self, n: &str) -> Option<&Register> {
        self.registers.iter().find(|r| r.name == n)
    }
    /// Registers gated by the named lock register.
    pub fn gated_by(&self, lock: &str) -> Vec<&Register> {
        self.registers
            .iter()
            .filter(|r| r.regwen.as_deref() == Some(lock))
            .collect()
    }
}

fn s(v: &serde_json::Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string())
}

/// Load an hjson file as JSON.
///
/// The fast path is the pure-Rust `deser-hjson` parser. Some in-tree specs use
/// constructs it rejects, so we fall back to the Python `hjson` module — the
/// same parser OpenTitan's own regtool uses — and consume its JSON output.
fn load_json(path: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    if let Ok(v) = deser_hjson::from_str::<serde_json::Value>(&text) {
        return Ok(v);
    }
    let out = std::process::Command::new("python3")
        .arg("-c")
        .arg("import hjson,json,sys; json.dump(hjson.load(open(sys.argv[1])), sys.stdout)")
        .arg(path)
        .output()
        .map_err(|e| format!("read {path:?}: hjson fallback unavailable: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "read {path:?}: hjson fallback failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("read {path:?}: {e}"))
}

/// Parse `bits: "7:4"` / `bits: "0"` into (msb, lsb).
fn parse_bits(bits: &str) -> (Option<u32>, Option<u32>) {
    let b = bits.trim();
    if let Some((h, l)) = b.split_once(':') {
        (
            h.trim().parse::<u32>().ok(),
            l.trim().parse::<u32>().ok(),
        )
    } else {
        let v = b.parse::<u32>().ok();
        (v, v)
    }
}

/// A field is treated as a multi-bit-boolean / locking encoding when the hjson
/// marks it as such, or its name/description carries the standard vocabulary.
fn looks_mubi(v: &serde_json::Value, name: &str) -> bool {
    if v.get("mubi").and_then(|x| x.as_bool()).unwrap_or(false) {
        return true;
    }
    let n = name.to_ascii_lowercase();
    n.contains("mubi")
        || n.ends_with("_en")
        || n.contains("fips")
        || n.contains("oneway")
        || n.contains("_lock")
        || n.contains("regwen")
        || n.contains("enable")
}

fn parse_resval(v: &serde_json::Value) -> Option<u64> {
    let r = v.get("resval")?;
    match r {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(t) => {
            let t = t.trim();
            if let Some(h) = t.strip_prefix("0x") {
                u64::from_str_radix(&h.replace('_', ""), 16).ok()
            } else {
                t.replace('_', "").parse::<u64>().ok()
            }
        }
        _ => None,
    }
}

/// Parse a generated register package for `PARAM_OFFSET = <size>'h <hex>` pairs.
fn parse_offsets(reg_pkg: &str) -> BTreeMap<String, u64> {
    let re = Regex::new(r"(?m)parameter\s+logic\s*\[[^\]]*\]\s*(\w+)_OFFSET\s*=\s*[^;]*?h\s*([0-9a-fA-F]+)\s*;")
        .expect("offset regex");
    let mut out = BTreeMap::new();
    for c in re.captures_iter(reg_pkg) {
        let name = &c[1];
        if let Ok(v) = u64::from_str_radix(&c[2], 16) {
            // Strip the IP-name prefix so the key matches the hjson register name.
            let key = name.rsplitn(2, '_').nth(1).map(|_| name).unwrap_or(name);
            out.insert(key.to_string(), v);
        }
    }
    out
}

/// Find the offset for a register whose hjson name is `reg`, given offsets keyed
/// by the generated `<IP>_<REG>` parameter base name.
fn offset_for(ip: &str, reg: &str, offsets: &BTreeMap<String, u64>) -> Option<u64> {
    let want = format!("{}_{}", ip.to_ascii_uppercase(), reg.to_ascii_uppercase());
    if let Some(v) = offsets.get(&want) {
        return Some(*v);
    }
    // Tolerate name-shape differences: match by suffix.
    let suffix = format!("_{}", reg.to_ascii_uppercase());
    offsets
        .iter()
        .find(|(k, _)| k.ends_with(&suffix))
        .map(|(_, v)| *v)
}

pub fn discover(tree: &Path, ip: &str) -> Result<Surface, String> {
    let mut hjson_path: Option<PathBuf> = None;
    let mut regpkg_path: Option<PathBuf> = None;

    for base in [
        tree.join("hw/ip").join(ip),
        tree.join("hw/top_earlgrey/ip_autogen").join(ip),
        tree.join("hw/ip_templates").join(ip),
    ] {
        if hjson_path.is_none() {
            let p = base.join("data").join(format!("{ip}.hjson"));
            if p.is_file() {
                hjson_path = Some(p);
            }
        }
        if regpkg_path.is_none() {
            let p = base.join("rtl").join(format!("{ip}_reg_pkg.sv"));
            if p.is_file() {
                regpkg_path = Some(p);
            }
        }
    }

    let mut registers: Vec<Register> = Vec::new();

    if let Some(hp) = &hjson_path {
        let v: serde_json::Value = load_json(hp)?;

        // OpenTitan uses `registers` in current hjson; older/other files use `regs`.
        let regs_arr = v
            .get("registers")
            .and_then(|r| r.as_array())
            .or_else(|| v.get("regs").and_then(|r| r.as_array()));
        if let Some(regs) = regs_arr {
            for r in regs {
                let name = match s(r, "name") {
                    Some(n) => n,
                    None => continue,
                };
                let mut fields = Vec::new();
                if let Some(fs) = r.get("fields").and_then(|f| f.as_array()) {
                    for f in fs {
                        let fname = match s(f, "name") {
                            Some(n) => n,
                            None => continue,
                        };
                        let bits = s(f, "bits").unwrap_or_else(|| "0".into());
                        let (msb, lsb) = parse_bits(&bits);
                        fields.push(Field {
                            is_mubi: looks_mubi(f, &fname),
                            resval: parse_resval(f),
                            swaccess: s(f, "swaccess"),
                            desc: s(f, "desc"),
                            name: fname,
                            bits,
                            msb,
                            lsb,
                        });
                    }
                }
                registers.push(Register {
                    name,
                    offset: None,
                    swaccess: s(r, "swaccess"),
                    hwaccess: s(r, "hwaccess"),
                    regwen: s(r, "regwen"),
                    fields,
                });
            }
        }
    }

    let mut offsets = BTreeMap::new();
    if let Some(rp) = &regpkg_path {
        let text = std::fs::read_to_string(rp).map_err(|e| format!("read {rp:?}: {e}"))?;
        offsets = parse_offsets(&text);
    }

    for r in registers.iter_mut() {
        r.offset = offset_for(ip, &r.name, &offsets);
    }

    // Merge generated offsets that have no hjson register: IPs mid-migration to
    // SystemRDL keep only part of their map in hjson, so the generated register
    // package is the complete list. Field-level detail is absent for the merged
    // (offset-only) entries and is reported as such.
    if !offsets.is_empty() {
        let pfx = format!("{}_", ip.to_ascii_uppercase());
        let known: Vec<String> = registers
            .iter()
            .map(|r| r.name.to_ascii_uppercase())
            .collect();
        for (k, v) in offsets.iter() {
            let name = k.strip_prefix(&pfx).unwrap_or(k).to_string();
            if !known.contains(&name.to_ascii_uppercase()) {
                registers.push(Register {
                    name,
                    offset: Some(*v),
                    swaccess: None,
                    hwaccess: None,
                    regwen: None,
                    fields: Vec::new(),
                });
            }
        }
    }
    registers.sort_by_key(|r| r.offset.unwrap_or(u64::MAX));

    // Registers that act as locks: named *_REGWEN, or referenced as another
    // register's `regwen`.
    let mut locks: Vec<String> = registers
        .iter()
        .filter(|r| r.name.ends_with("_REGWEN"))
        .map(|r| r.name.clone())
        .collect();
    for r in registers.iter() {
        if let Some(l) = &r.regwen {
            if !locks.contains(l) {
                locks.push(l.clone());
            }
        }
    }

    if registers.is_empty() {
        return Err(format!(
            "no registers discovered for {ip}: hjson={:?} reg_pkg={:?}",
            hjson_path, regpkg_path
        ));
    }

    Ok(Surface {
        ip: ip.to_string(),
        tree: tree.display().to_string(),
        hjson: hjson_path.map(|p| p.display().to_string()),
        reg_pkg: regpkg_path.map(|p| p.display().to_string()),
        registers,
        lock_registers: locks,
    })
}

/// Every IP for which a register spec can be discovered.
pub fn discover_all(tree: &Path) -> Vec<Surface> {
    let mut names: Vec<String> = Vec::new();
    for root in [
        tree.join("hw/ip"),
        tree.join("hw/top_earlgrey/ip_autogen"),
        tree.join("hw/ip_templates"),
    ] {
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    if !names.contains(&n.to_string()) {
                        names.push(n.to_string());
                    }
                }
            }
        }
    }
    names.sort();
    names
        .into_iter()
        .filter_map(|n| discover(tree, &n).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_parse() {
        assert_eq!(parse_bits("7:4"), (Some(7), Some(4)));
        assert_eq!(parse_bits("0"), (Some(0), Some(0)));
    }

    #[test]
    fn offsets_parse() {
        let pkg = r#"
          parameter logic [BlockAw-1:0] AES_ALERT_TEST_OFFSET = 8'h 0;
          parameter logic [BlockAw-1:0] AES_KEY_SHARE0_0_OFFSET = 8'h 4;
          parameter logic [BlockAw-1:0] AES_CTRL_AUX_REGWEN_OFFSET = 8'h 1c;
        "#;
        let m = parse_offsets(pkg);
        assert_eq!(m.get("AES_ALERT_TEST"), Some(&0));
        assert_eq!(m.get("AES_KEY_SHARE0_0"), Some(&0x4));
        assert_eq!(m.get("AES_CTRL_AUX_REGWEN"), Some(&0x1c));
    }

    #[test]
    fn offset_lookup_by_suffix() {
        let mut m = BTreeMap::new();
        m.insert("AES_CTRL".to_string(), 0x18u64);
        assert_eq!(offset_for("aes", "CTRL", &m), Some(0x18));
    }
}
