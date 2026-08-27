use midi_forge_core::{
    IDENTITY_REQUEST, UmpMessage, decode, midi2_cc, midi2_note_on, panic_packets, value7_to_32,
    velocity7_to_16,
};

use super::host::{EndpointInfo, McpHost, format_ump_words};

pub struct SendNote {
    pub out: String,
    pub note: u8,
    pub vel: u8,
    pub ch: u8,
    pub group: u8,
    pub m2: bool,
}

pub struct SendCc {
    pub out: String,
    pub cc: u8,
    pub val: u8,
    pub ch: u8,
    pub group: u8,
    pub m2: bool,
}

pub struct Identity {
    pub out: String,
}

pub struct Panic {
    pub out: Option<String>,
}

pub struct SetPortOpen {
    pub id: String,
    pub output: bool,
    pub open: bool,
}

fn require_armed(host: &dyn McpHost) -> Result<(), String> {
    if host.armed() {
        Ok(())
    } else {
        Err("writes disabled until arm".into())
    }
}

fn clamp_limit(limit: usize) -> usize {
    if limit == 0 { 40 } else { limit.clamp(1, 200) }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn describe_packet(packet: &UmpMessage) -> String {
    format!("{}  {}", decode(packet).summary(), format_ump_words(packet))
}

fn find_output(host: &dyn McpHost, needle: &str) -> Result<EndpointInfo, String> {
    let n = needle.to_ascii_lowercase();
    host.list_endpoints()
        .into_iter()
        .filter(|e| e.direction == "out" || e.direction == "bidi")
        .find(|e| {
            e.name.to_ascii_lowercase().contains(&n) || e.id.to_ascii_lowercase().contains(&n)
        })
        .ok_or_else(|| format!("no output matching {needle:?}"))
}

fn channel(ch: u8) -> u8 {
    ch.saturating_sub(1).min(15)
}

fn group(g: u8) -> u8 {
    g.min(15)
}

pub fn list_endpoints(host: &mut dyn McpHost) -> Result<String, String> {
    let eps = host.list_endpoints();
    let mut s = String::from("[");
    for (i, e) in eps.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"id\":\"{}\",\"name\":\"{}\",\"direction\":\"{}\",\"protocol\":\"{}\",\"open\":{}}}",
            json_escape(&e.id),
            json_escape(&e.name),
            json_escape(&e.direction),
            json_escape(&e.protocol),
            e.open
        ));
    }
    s.push(']');
    Ok(s)
}

pub fn monitor_tail(host: &mut dyn McpHost, limit: usize) -> Result<String, String> {
    let rows = host.monitor_tail(clamp_limit(limit));
    let mut s = String::from("[");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"time_ns\":{},\"port\":\"{}\",\"ump_words\":\"{}\",\"decoded\":\"{}\"}}",
            row.time_ns,
            json_escape(&row.port),
            json_escape(&row.ump_words),
            json_escape(&row.decoded)
        ));
    }
    s.push(']');
    Ok(s)
}

pub fn live_now(host: &mut dyn McpHost) -> Result<String, String> {
    Ok(host.live_dump())
}

pub fn clock_health(host: &mut dyn McpHost) -> Result<String, String> {
    Ok(host.clock_summary())
}

pub fn stuck_notes(host: &mut dyn McpHost) -> Result<String, String> {
    let notes = host.stuck_notes();
    if notes.is_empty() {
        Ok("none".into())
    } else {
        Ok(notes.join("\n"))
    }
}

pub fn thru_graph(host: &mut dyn McpHost) -> Result<String, String> {
    Ok(host.thru_graph())
}

pub fn mpe_status(host: &mut dyn McpHost) -> Result<String, String> {
    Ok(host.mpe_status())
}

pub fn snapshot(host: &mut dyn McpHost) -> Result<String, String> {
    Ok(host.snapshot())
}

pub fn send_note(host: &mut dyn McpHost, args: SendNote) -> Result<String, String> {
    require_armed(host)?;
    let ep = find_output(host, &args.out)?;
    let ch = channel(args.ch);
    let group = group(args.group);
    let note = args.note.min(127);
    let vel = args.vel.min(127);
    let packet = if args.m2 {
        midi2_note_on(group, ch, note, velocity7_to_16(vel))
    } else {
        UmpMessage::midi1_channel_voice(group, 0x90 | ch, note, vel)
    };
    host.send(&ep.id, &packet)?;
    Ok(format!("{} → {}", describe_packet(&packet), ep.name))
}

pub fn send_cc(host: &mut dyn McpHost, args: SendCc) -> Result<String, String> {
    require_armed(host)?;
    let ep = find_output(host, &args.out)?;
    let ch = channel(args.ch);
    let group = group(args.group);
    let cc = args.cc.min(127);
    let val = args.val.min(127);
    let packet = if args.m2 {
        midi2_cc(group, ch, cc, value7_to_32(val))
    } else {
        UmpMessage::midi1_channel_voice(group, 0xB0 | ch, cc, val)
    };
    host.send(&ep.id, &packet)?;
    Ok(format!("{} → {}", describe_packet(&packet), ep.name))
}

pub fn identity(host: &mut dyn McpHost, args: Identity) -> Result<String, String> {
    require_armed(host)?;
    let ep = find_output(host, &args.out)?;
    host.send_sysex(&ep.id, &IDENTITY_REQUEST)?;
    let payload = &IDENTITY_REQUEST[1..IDENTITY_REQUEST.len() - 1];
    let ump = UmpMessage::sysex7(0, 0, payload);
    Ok(format!(
        "Identity Request  {} → {}",
        describe_packet(&ump),
        ep.name
    ))
}

pub fn panic(host: &mut dyn McpHost, args: Panic) -> Result<String, String> {
    require_armed(host)?;
    let dests: Vec<EndpointInfo> = if let Some(out) = args.out.as_deref() {
        vec![find_output(host, out)?]
    } else {
        let open = host.open_outputs();
        let eps = host.list_endpoints();
        let mut dests: Vec<EndpointInfo> = if open.is_empty() {
            eps.into_iter()
                .filter(|e| e.direction == "out" || e.direction == "bidi")
                .collect()
        } else {
            eps.into_iter()
                .filter(|e| open.iter().any(|id| id == &e.id))
                .collect()
        };
        if dests.is_empty() {
            return Err("no outputs to panic".into());
        }
        dests.sort_by(|a, b| a.id.cmp(&b.id));
        dests
    };
    let packets = panic_packets();
    let mut sent = 0usize;
    for dest in &dests {
        for packet in &packets {
            host.send(&dest.id, packet)?;
            sent += 1;
        }
    }
    let sample = packets
        .first()
        .map(describe_packet)
        .unwrap_or_else(|| "panic".into());
    let names: Vec<&str> = dests.iter().map(|e| e.name.as_str()).collect();
    Ok(format!(
        "panic {sent} packets → {}  {sample}",
        names.join(", ")
    ))
}

pub fn set_port_open(host: &mut dyn McpHost, args: SetPortOpen) -> Result<String, String> {
    require_armed(host)?;
    host.set_port_open(&args.id, args.output, args.open)?;
    let dir = if args.output { "output" } else { "input" };
    let state = if args.open { "opened" } else { "closed" };
    Ok(format!("{state} {dir} {}", args.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::host::StandaloneHost;

    #[test]
    fn monitor_tail_returns_decoded_note() {
        let mut host = StandaloneHost::with_null();
        host.push_note();
        let json = crate::mcp::tools::monitor_tail(&mut host, 10).unwrap();
        assert!(json.contains("NoteOn"));
        assert!(json.contains("2090") || json.contains("2090_3C64") || json.contains("3C64"));
    }

    #[test]
    fn send_note_refuses_when_unarmed() {
        let mut host = StandaloneHost::with_null();
        let err = crate::mcp::tools::send_note(
            &mut host,
            crate::mcp::tools::SendNote {
                out: "Null Synth".into(),
                note: 60,
                vel: 100,
                ch: 1,
                group: 0,
                m2: false,
            },
        )
        .unwrap_err();
        assert!(err.to_lowercase().contains("arm"));
        assert!(host.sent().is_empty());
    }

    #[test]
    fn send_note_records_when_armed() {
        let mut host = StandaloneHost::with_null();
        host.set_armed(true);
        let out = crate::mcp::tools::send_note(
            &mut host,
            crate::mcp::tools::SendNote {
                out: "Null Synth".into(),
                note: 60,
                vel: 100,
                ch: 1,
                group: 0,
                m2: false,
            },
        )
        .unwrap();
        assert!(out.contains("NoteOn"));
        assert!(out.contains("2090") || out.contains("2090_3C64") || out.contains("3C64"));
        assert_eq!(host.sent().len(), 1);
        assert_eq!(host.sent()[0].0, "null:out:0");
        assert_eq!(
            host.sent()[0].1,
            UmpMessage::midi1_channel_voice(0, 0x90, 60, 100)
        );
    }

    #[test]
    fn write_tools_require_arm() {
        let mut host = StandaloneHost::with_null();
        let cc = crate::mcp::tools::send_cc(
            &mut host,
            SendCc {
                out: "Null Synth".into(),
                cc: 1,
                val: 64,
                ch: 1,
                group: 0,
                m2: false,
            },
        )
        .unwrap_err();
        assert!(cc.to_lowercase().contains("arm"));
        let id = crate::mcp::tools::identity(
            &mut host,
            Identity {
                out: "Null Synth".into(),
            },
        )
        .unwrap_err();
        assert!(id.to_lowercase().contains("arm"));
        let pan = crate::mcp::tools::panic(&mut host, Panic { out: None }).unwrap_err();
        assert!(pan.to_lowercase().contains("arm"));
        let port = crate::mcp::tools::set_port_open(
            &mut host,
            SetPortOpen {
                id: "null:out:0".into(),
                output: true,
                open: true,
            },
        )
        .unwrap_err();
        assert!(port.to_lowercase().contains("arm"));
    }

    #[test]
    fn list_endpoints_includes_fixture_ports() {
        let mut host = StandaloneHost::with_null();
        let json = crate::mcp::tools::list_endpoints(&mut host).unwrap();
        assert!(json.contains("Null Keyboard"));
        assert!(json.contains("Null Synth"));
        assert!(json.contains("null:in:0"));
        assert!(json.contains("null:out:0"));
    }

    #[test]
    fn thru_graph_none_when_empty() {
        let mut host = StandaloneHost::with_null();
        let text = crate::mcp::tools::thru_graph(&mut host).unwrap();
        assert!(text.to_lowercase().contains("none"));
    }

    #[test]
    fn live_and_stuck_after_push_note() {
        let mut host = StandaloneHost::with_null();
        host.push_note();
        let live = crate::mcp::tools::live_now(&mut host).unwrap();
        assert!(live.contains("note 60") || live.contains("60"));
        let stuck = crate::mcp::tools::stuck_notes(&mut host).unwrap();
        assert!(stuck.contains("60"));
        let snap = crate::mcp::tools::snapshot(&mut host).unwrap();
        assert!(snap.contains("Stuck notes"));
        let clock = crate::mcp::tools::clock_health(&mut host).unwrap();
        assert!(!clock.is_empty());
        let mpe = crate::mcp::tools::mpe_status(&mut host).unwrap();
        assert!(mpe.to_lowercase().contains("mpe"));
    }

    #[test]
    fn identity_sends_sysex_when_armed() {
        let mut host = StandaloneHost::with_null();
        host.set_armed(true);
        let out = crate::mcp::tools::identity(
            &mut host,
            Identity {
                out: "Null Synth".into(),
            },
        )
        .unwrap();
        assert!(out.contains("Identity") || out.contains("SysEx"));
        assert!(out.contains("307E") || out.contains("7E7F") || out.contains("7E"));
        assert_eq!(host.sent_sysex().len(), 1);
        assert_eq!(host.sent_sysex()[0].1, IDENTITY_REQUEST.to_vec());
    }

    #[test]
    fn send_cc_and_panic_record_when_armed() {
        let mut host = StandaloneHost::with_null();
        host.set_armed(true);
        let cc = crate::mcp::tools::send_cc(
            &mut host,
            SendCc {
                out: "synth".into(),
                cc: 1,
                val: 64,
                ch: 1,
                group: 0,
                m2: false,
            },
        )
        .unwrap();
        assert!(
            cc.contains("2090") || cc.contains("20B0") || cc.contains("B0") || cc.contains("1")
        );
        assert_eq!(host.sent().len(), 1);
        crate::mcp::tools::panic(
            &mut host,
            Panic {
                out: Some("Null Synth".into()),
            },
        )
        .unwrap();
        assert!(host.sent().len() > 1);
    }

    #[test]
    fn set_port_open_toggles_output() {
        let mut host = StandaloneHost::with_null();
        host.set_armed(true);
        crate::mcp::tools::set_port_open(
            &mut host,
            SetPortOpen {
                id: "null:out:0".into(),
                output: true,
                open: true,
            },
        )
        .unwrap();
        assert!(host.open_outputs().iter().any(|id| id == "null:out:0"));
        crate::mcp::tools::set_port_open(
            &mut host,
            SetPortOpen {
                id: "Null Synth".into(),
                output: true,
                open: false,
            },
        )
        .unwrap();
        assert!(host.open_outputs().is_empty());
    }

    #[test]
    fn monitor_tail_clamps_to_last_n() {
        let mut host = StandaloneHost::with_null();
        for _ in 0..5 {
            host.push_note();
        }
        let json = crate::mcp::tools::monitor_tail(&mut host, 2).unwrap();
        assert_eq!(json.matches("NoteOn").count(), 2);
        let all = crate::mcp::tools::monitor_tail(&mut host, 0).unwrap();
        assert_eq!(all.matches("NoteOn").count(), 5);
    }
}
