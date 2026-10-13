//! campaign.rs — generate command sequences for a generated harness.
//!
//! The harness replays a flat command list read with `$readmemh`: three hex
//! tokens per command (op, address, data), terminated by a sentinel whose
//! address is SENTINEL. op 0 = write, 1 = read, 2 = idle N cycles.

use crate::surface::Surface;

pub const SENTINEL: u32 = 0xdead_beef;

pub fn sentinel() -> String {
    format!("2\n{:x}\n{:x}\n", SENTINEL, 0)
}

/// Read every register that has a discovered offset. This is the connectivity
/// check: if the bus and the generated harness work, these reads come back with
/// real data and no error flags.
pub fn smoke(s: &Surface) -> String {
    let mut out = String::new();
    for r in &s.registers {
        if let Some(off) = r.offset {
            // op 1 = read
            out.push_str(&format!("1\n{:x}\n0\n", off));
        }
    }
    out.push_str(&sentinel());
    out
}

/// OpenTitan's canonical 4-bit multi-bit-boolean encodings. Every other 4-bit
/// value is non-canonical: the design is supposed to reject it (or at least
/// report it), never quietly consent to it.
pub const MUBI4_TRUE: u32 = 0x6;
pub const MUBI4_FALSE: u32 = 0x9;

fn is_mubi4_width(f: &crate::surface::Field) -> bool {
    match (f.msb, f.lsb) {
        (Some(m), Some(l)) if m >= l => m - l + 1 == 4,
        _ => false,
    }
}

/// Build a register word in which every 4-bit (encoding-class) field holds a
/// canonical encoding, so that varying one field cannot be confounded by its
/// siblings being non-canonical. Fields that look like module enables are set
/// to canonical true: a design that is switched off is not a meaningful target,
/// and enables are what make the remaining controls reachable.
fn canonical_base(r: &crate::surface::Register) -> u32 {
    let mut word = 0u32;
    for f in &r.fields {
        if is_mubi4_width(f) {
            let lsb = f.lsb.unwrap();
            let n = f.name.to_ascii_uppercase();
            let enc = if n.contains("ENABLE") {
                MUBI4_TRUE
            } else {
                MUBI4_FALSE
            };
            word |= enc << lsb;
        }
    }
    word
}

/// Registers that gate writes to others (write-enable locks).
fn lock_registers(s: &Surface) -> Vec<u64> {
    s.registers
        .iter()
        .filter(|r| r.name.to_ascii_uppercase().ends_with("_REGWEN"))
        .filter_map(|r| r.offset)
        .collect()
}

/// One probe's bookkeeping: which commands in the stream carry the write, the
/// read-back, and each status register read. Every command produces exactly one
/// response line, so these indices locate the responses to correlate.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Probe {
    pub probe: usize,
    pub register: String,
    pub field: String,
    pub lsb: u32,
    pub value: u32,
    pub write_idx: usize,
    pub readback_idx: usize,
    /// Response index of the alert read taken AFTER clearing, BEFORE the
    /// write. Must read 0 for this probe to be scorable: if the alert cannot
    /// be cleared, the post-write value cannot be attributed to this write.
    pub status_before: usize,
    /// (status register name, command index) read after this probe.
    pub status: Vec<(String, usize)>,
}

/// Non-canonical encoding sweep with a detection-completeness oracle.
///
/// For every 4-bit encoding-class field, every one of the 16 possible values is
/// written through the real bus, then the field is read back and the design's
/// own alert/status registers are read. A value that is non-canonical yet
/// accepted, and that leaves the corresponding field alert clear, is an
/// undetected violation of the encoding contract — with no knowledge of any
/// specific defect required.
///
/// Returns the command stream and a JSON-lines probe map.
pub fn mubi_sweep(s: &Surface, only: Option<usize>) -> (String, String) {
    let mut cmds = String::new();
    let mut map = String::new();
    let status: Vec<(String, u64)> = s
        .registers
        .iter()
        .filter(|r| {
            let n = r.name.to_ascii_uppercase();
            n.contains("ALERT_STS") || n.contains("RECOV_ALERT") || n.contains("ERR_CODE")
        })
        .filter_map(|r| r.offset.map(|o| (r.name.clone(), o)))
        .collect();
    let mut probe = 0usize;
    // The sweep writes all 16 values to MODULE_ENABLE, so without re-asserting a
    // canonical enable the module is disabled for most probes and its CSRs stop
    // accepting writes - which is why a per-probe write-1-to-clear of
    // RECOV_ALERT_STS left 367 of 368 probes with a dirty baseline. Re-enable
    // (canonical) before touching the alert.
    let enable_reassert: Option<u32> = s
        .registers
        .iter()
        .filter(|r| r.offset.is_some())
        .find(|r| {
            r.fields.iter().any(|f| {
                is_mubi4_width(f) && f.name.to_ascii_uppercase().contains("ENABLE")
            })
        })
        .map(|r| canonical_base(r));
    let mut idx = 0usize;
    // Only writes and reads produce a response line; idle commands do not. The
    // probe map must therefore index responses, not commands.
    let mut resp = 0usize;

    // Precondition: unlock the write-enable locks. Without this every write to
    // a protected register is simply dropped and the sweep measures nothing.
    for off in lock_registers(s) {
        cmds.push_str(&format!("0\n{:x}\n1\n", off));
        idx += 1;
        resp += 1;
    }
    cmds.push_str("2\n0\n32\n");
    idx += 1;

    // Enable the design's own module-enable controls (canonical true), so the
    // controls under test are actually reachable.
    for r in &s.registers {
        let off = match r.offset {
            Some(o) => o,
            None => continue,
        };
        let word = canonical_base(r);
        let has_en = r.fields.iter().any(|f| {
            is_mubi4_width(f) && f.name.to_ascii_uppercase().contains("ENABLE")
        });
        if has_en && word != 0 {
            cmds.push_str(&format!("0\n{:x}\n{:x}\n", off, word));
            idx += 1;
            resp += 1;
        }
    }
    cmds.push_str("2\n0\n64\n");
    idx += 1;

    for r in &s.registers {
        let off = match r.offset {
            Some(o) => o,
            None => continue,
        };
        let mubi_fields: Vec<&crate::surface::Field> =
            r.fields.iter().filter(|f| is_mubi4_width(f)).collect();
        if mubi_fields.is_empty() {
            continue;
        }
        let base = canonical_base(r);

        for f in mubi_fields {
            let lsb = f.lsb.unwrap();
            for v in 0u32..16 {
                let word = (base & !(0xFu32 << lsb)) | (v << lsb);

                // One probe per run when `only` is set: the alert cannot be
                // cleared mid-run, so attribution needs a fresh reset per probe.
                let selected = only.map_or(true, |n| n == probe);
                if selected {
                // Re-assert a canonical module enable first: a previous probe
                // may have written a non-canonical value here and disabled the
                // block, in which case the clear below is silently dropped.
                if let Some(word) = enable_reassert {
                    if let Some(er) = s
                        .registers
                        .iter()
                        .find(|r| {
                            r.offset.is_some()
                                && r.fields.iter().any(|f| {
                                    is_mubi4_width(f)
                                        && f.name.to_ascii_uppercase().contains("ENABLE")
                                })
                        })
                        .and_then(|r| r.offset)
                    {
                        cmds.push_str(&format!("0\n{:x}\n{:x}\n", er, word));
                        idx += 1;
                        resp += 1;
                    }
                }

                // Clear the sticky alert bits BEFORE the probe.
                //
                // RECOV_ALERT_STS is write-1-to-clear and latches for the rest
                // of the run, so without this the status read after a probe
                // reflects every earlier probe's alerts too. The signatures then
                // accumulate across the sweep (0x4 -> 0x20005 -> 0x60005 ->
                // 0x60c3f) and "same status as a canonical write" measures
                // history rather than the value under test - which makes an
                // undetected violation and a correctly-alerted one look alike.
                for (name, a) in &status {
                    if name.to_ascii_uppercase().contains("ALERT_STS") {
                        cmds.push_str(&format!("0\n{:x}\nffffffff\n", a));
                        idx += 1;
                        resp += 1;
                    }
                }
                // Let the clear settle, then read the alert back. This read is
                // the probe's self-check: a NONZERO value here means the alert
                // could not be cleared, so whatever the post-write read shows is
                // contaminated by earlier probes and the probe is unscorable.
                cmds.push_str("2\n0\n8\n");
                idx += 1;
                let status_before = resp;
                for (name, a) in &status {
                    if name.to_ascii_uppercase().contains("ALERT_STS") {
                        cmds.push_str(&format!("1\n{:x}\n0\n", a));
                        idx += 1;
                        resp += 1;
                    }
                }

                let write_idx = resp;
                cmds.push_str(&format!("0\n{:x}\n{:x}\n", off, word));
                idx += 1;
                resp += 1;

                cmds.push_str("2\n0\n16\n");
                idx += 1;

                let readback_idx = resp;
                cmds.push_str(&format!("1\n{:x}\n0\n", off));
                idx += 1;
                resp += 1;

                let mut st = Vec::new();
                for (name, a) in &status {
                    st.push((name.clone(), resp));
                    cmds.push_str(&format!("1\n{:x}\n0\n", a));
                    idx += 1;
                    resp += 1;
                }

                let p = Probe {
                    probe,
                    register: r.name.clone(),
                    field: f.name.clone(),
                    lsb,
                    value: v,
                    write_idx,
                    readback_idx,
                    status_before,
                    status: st,
                };
                map.push_str(&serde_json::to_string(&p).expect("serialize probe"));
                map.push('\n');
                }
                probe += 1;
            }
        }
    }
    cmds.push_str(&sentinel());
    (cmds, map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{Field, Register};

    fn surface_with(offsets: &[u64]) -> Surface {
        Surface {
            ip: "t".into(),
            tree: "/t".into(),
            hjson: None,
            reg_pkg: None,
            registers: offsets
                .iter()
                .enumerate()
                .map(|(i, o)| Register {
                    name: format!("R{i}"),
                    offset: Some(*o),
                    swaccess: None,
                    hwaccess: None,
                    regwen: None,
                    fields: Vec::<Field>::new(),
                })
                .collect(),
            lock_registers: vec![],
        }
    }

    #[test]
    fn smoke_emits_reads_then_sentinel() {
        let s = surface_with(&[0x0, 0x10]);
        let out = smoke(&s);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "1");
        assert_eq!(lines[1], "0");
        assert_eq!(lines[3], "1");
        assert_eq!(lines[4], "10");
        // last three lines are the sentinel
        assert_eq!(lines[lines.len() - 3], "2");
        assert_eq!(lines[lines.len() - 2], "deadbeef");
    }

    #[test]
    fn registers_without_offsets_are_skipped() {
        let mut s = surface_with(&[0x4]);
        s.registers.push(Register {
            name: "NOOFF".into(),
            offset: None,
            swaccess: None,
            hwaccess: None,
            regwen: None,
            fields: vec![],
        });
        let out = smoke(&s);
        let n_reads = out.lines().filter(|l| *l == "1").count();
        assert_eq!(n_reads, 1);
    }
}
