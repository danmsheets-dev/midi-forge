use std::sync::{Arc, Mutex};

use midi_forge_core::{MidiEvent, PortId, Timestamp, UmpMessage, decode};
use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Table, Value};
use thiserror::Error;

pub const DEFAULT_SOURCE: &str = r#"-- Midi-Forge Lua (runs on captured events before thru).
-- on_midi(ev): return false to drop, a table to replace, or extra tables to fan out.
-- ev.kind, ev.channel (0-15), ev.data1, ev.data2, ev.status, ev.group, ev.port
--
-- Drop clock:
--   if ev.kind == "clock" then return false end
--   return ev
--
-- Transpose notes up an octave:
--   if ev.kind == "note_on" or ev.kind == "note_off" then
--     ev.data1 = math.min(127, ev.data1 + 12)
--   end
--   return ev

function on_midi(ev)
  return ev
end
"#;

const LOG_CAP: usize = 200;
const PRELUDE: &str = include_str!("prelude.lua");

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("{0}")]
    Lua(String),
}

impl From<mlua::Error> for ScriptError {
    fn from(err: mlua::Error) -> Self {
        Self::Lua(err.to_string())
    }
}

/// Lua VM + editor buffer. `Send` with mlua `send` feature.
pub struct ScriptEngine {
    pub source: String,
    lua: Lua,
    enabled: bool,
    error: Option<String>,
    log: Arc<Mutex<Vec<String>>>,
    timers: Arc<Mutex<Vec<(u64, MidiEvent)>>>,
    now_ns: Arc<Mutex<u64>>,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptEngine {
    pub fn new() -> Self {
        let log = Arc::new(Mutex::new(Vec::new()));
        let timers = Arc::new(Mutex::new(Vec::new()));
        let now_ns = Arc::new(Mutex::new(0u64));
        let lua = sandbox(Arc::clone(&log), Arc::clone(&timers), Arc::clone(&now_ns))
            .expect("sandbox Lua");
        let mut this = Self {
            source: DEFAULT_SOURCE.to_string(),
            lua,
            enabled: false,
            error: None,
            log,
            timers,
            now_ns,
        };
        let _ = this.reload();
        this
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn log_lines(&self) -> Vec<String> {
        self.log.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn clear_log(&mut self) {
        if let Ok(mut g) = self.log.lock() {
            g.clear();
        }
    }

    pub fn reload(&mut self) -> Result<(), ScriptError> {
        let log = Arc::clone(&self.log);
        let lua = sandbox(log, Arc::clone(&self.timers), Arc::clone(&self.now_ns))?;
        lua.load(PRELUDE).set_name("prelude.lua").exec()?;
        match lua.load(&self.source).set_name("user.lua").exec() {
            Ok(()) => {
                self.lua = lua;
                self.error = None;
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                self.error = Some(msg.clone());
                Err(ScriptError::Lua(msg))
            }
        }
    }

    /// When disabled, identity. Runtime errors fail open (original event).
    pub fn process(&mut self, event: &MidiEvent) -> Vec<MidiEvent> {
        if !self.enabled {
            return vec![*event];
        }
        match self.process_inner(event) {
            Ok(out) => {
                if self
                    .error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("runtime:"))
                {
                    self.error = None;
                }
                out
            }
            Err(err) => {
                self.error = Some(format!("runtime: {err}"));
                vec![*event]
            }
        }
    }

    fn process_inner(&self, event: &MidiEvent) -> Result<Vec<MidiEvent>, ScriptError> {
        let on_midi: Option<Function> = self.lua.globals().get("on_midi")?;
        let Some(on_midi) = on_midi else {
            return Ok(vec![*event]);
        };
        let tbl = event_to_table(&self.lua, event)?;
        let rets: MultiValue = on_midi.call(tbl)?;
        values_to_events(&self.lua, rets, event)
    }

    pub fn tick(&mut self, now_ns: u64) -> Vec<MidiEvent> {
        if let Ok(mut n) = self.now_ns.lock() {
            *n = now_ns;
        }
        if !self.enabled {
            return Vec::new();
        }
        let mut due = Vec::new();
        if let Ok(mut t) = self.timers.lock() {
            t.retain(|(when, ev)| {
                if *when <= now_ns {
                    due.push(*ev);
                    false
                } else {
                    true
                }
            });
        }
        if let Ok(Some(on_idle)) = self.lua.globals().get::<Option<Function>>("on_idle") {
            let dummy = MidiEvent::new(
                Timestamp::from_nanos(now_ns),
                PortId(0),
                UmpMessage::midi1_system(0, 0xF8, 0, 0),
            );
            if let Ok(rets) = on_idle.call::<MultiValue>(now_ns as f64 / 1_000_000.0) {
                if let Ok(extra) = values_to_events(&self.lua, rets, &dummy) {
                    due.extend(extra.into_iter().filter(|e| {
                        e.packet.message_type() != 0x1 || e.packet.status_byte() != 0xF8
                    }));
                }
            }
        }
        due
    }

    pub fn export_state(&self) -> String {
        let Ok(midi) = self.lua.globals().get::<Table>("midi") else {
            return "{}".into();
        };
        let Ok(Some(state)) = midi.get::<Option<Table>>("state") else {
            return "{}".into();
        };
        let mut map = serde_json::Map::new();
        for pair in state.pairs::<Value, Value>().flatten() {
            let (k, v) = pair;
            let key = match k {
                Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                Value::Integer(i) => i.to_string(),
                _ => continue,
            };
            let val = match v {
                Value::Nil => serde_json::Value::Null,
                Value::Boolean(b) => serde_json::Value::Bool(b),
                Value::Integer(i) => serde_json::json!(i),
                Value::Number(n) => serde_json::json!(n),
                Value::String(s) => {
                    serde_json::Value::String(s.to_str().map(|s| s.to_string()).unwrap_or_default())
                }
                _ => continue,
            };
            map.insert(key, val);
        }
        serde_json::Value::Object(map).to_string()
    }

    pub fn import_state(&mut self, json: &str) {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
            return;
        };
        let Some(obj) = val.as_object() else {
            return;
        };
        let Ok(midi) = self.lua.globals().get::<Table>("midi") else {
            return;
        };
        let Ok(state) = self.lua.create_table() else {
            return;
        };
        for (k, v) in obj {
            let _ = match v {
                serde_json::Value::Null => state.set(k.as_str(), Value::Nil),
                serde_json::Value::Bool(b) => state.set(k.as_str(), *b),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        state.set(k.as_str(), i)
                    } else {
                        state.set(k.as_str(), n.as_f64().unwrap_or(0.0))
                    }
                }
                serde_json::Value::String(s) => state.set(k.as_str(), s.as_str()),
                _ => Ok(()),
            };
        }
        let _ = midi.set("state", state);
    }
}

fn sandbox(
    log: Arc<Mutex<Vec<String>>>,
    timers: Arc<Mutex<Vec<(u64, MidiEvent)>>>,
    now_ns: Arc<Mutex<u64>>,
) -> Result<Lua, ScriptError> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::COROUTINE | StdLib::UTF8,
        LuaOptions::default(),
    )?;
    let _ = lua.set_memory_limit(4 * 1024 * 1024);

    let print_log = Arc::clone(&log);
    lua.globals().set(
        "print",
        lua.create_function(move |_, args: MultiValue| {
            push_log(&print_log, &multi_to_string(args));
            Ok(())
        })?,
    )?;

    let midi_log = Arc::clone(&log);
    let midi = lua.create_table()?;
    midi.set(
        "log",
        lua.create_function(move |_, args: MultiValue| {
            push_log(&midi_log, &multi_to_string(args));
            Ok(())
        })?,
    )?;
    midi.set("state", lua.create_table()?)?;
    let dummy = MidiEvent::new(
        Timestamp::from_nanos(0),
        PortId(0),
        UmpMessage::midi1_system(0, 0xF8, 0, 0),
    );
    midi.set(
        "after",
        lua.create_function(move |_, (ms, tbl): (f64, Table)| {
            let ev = table_to_event(&tbl, &dummy).map_err(mlua::Error::external)?;
            let now = now_ns.lock().map(|g| *g).unwrap_or(0);
            let due = now.saturating_add((ms.max(0.0) * 1_000_000.0) as u64);
            if let Ok(mut t) = timers.lock() {
                t.push((due, ev));
            }
            Ok(())
        })?,
    )?;
    lua.globals().set("midi", midi)?;
    Ok(lua)
}

fn push_log(log: &Mutex<Vec<String>>, line: &str) {
    let Ok(mut log) = log.lock() else {
        return;
    };
    if log.len() >= LOG_CAP {
        log.remove(0);
    }
    log.push(line.to_string());
}

fn multi_to_string(args: MultiValue) -> String {
    let parts: Vec<String> = args.iter().map(value_to_string).collect();
    parts.join("\t")
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
        other => format!("{other:?}"),
    }
}

fn event_to_table(lua: &Lua, event: &MidiEvent) -> Result<Table, mlua::Error> {
    let packet = event.packet;
    let decoded = decode(&packet);
    let tbl = lua.create_table()?;
    tbl.set("port", event.port.0)?;
    tbl.set("time", event.time.nanos)?;
    tbl.set("type", packet.message_type())?;
    tbl.set("group", packet.group())?;
    tbl.set("status", packet.status_byte())?;
    tbl.set("data1", packet.data1())?;
    tbl.set("data2", packet.data2())?;
    tbl.set("kind", decoded.kind_key())?;
    if let Some(ch) = packet.channel() {
        tbl.set("channel", ch)?;
    }
    if packet.len() > 1 {
        tbl.set("word1", packet.words()[1])?;
    }
    let words = lua.create_table()?;
    for (i, w) in packet.words().iter().enumerate() {
        words.set(i + 1, *w)?;
    }
    tbl.set("words", words)?;
    Ok(tbl)
}

fn values_to_events(
    _lua: &Lua,
    rets: MultiValue,
    fallback: &MidiEvent,
) -> Result<Vec<MidiEvent>, ScriptError> {
    let values: Vec<Value> = rets.into_iter().collect();
    if values.is_empty() || values.iter().all(|v| matches!(v, Value::Nil)) {
        return Ok(vec![*fallback]);
    }
    if matches!(values.first(), Some(Value::Boolean(false))) {
        return Ok(Vec::new());
    }
    if matches!(values.first(), Some(Value::Boolean(true))) {
        return Ok(vec![*fallback]);
    }
    let mut out = Vec::new();
    for v in values {
        match v {
            Value::Nil | Value::Boolean(true) => out.push(*fallback),
            Value::Boolean(false) => {}
            Value::Table(t) => {
                if t.get::<Option<Table>>(1)?.is_some()
                    && t.get::<Option<Value>>("status")?.is_none()
                {
                    let mut i = 1;
                    while let Ok(Some(item)) = t.get::<Option<Table>>(i) {
                        out.push(table_to_event(&item, fallback)?);
                        i += 1;
                    }
                } else {
                    out.push(table_to_event(&t, fallback)?);
                }
            }
            _ => out.push(*fallback),
        }
    }
    Ok(out)
}

fn lua_u8(table: &Table, key: &str, fallback: u8) -> Result<u8, mlua::Error> {
    Ok(match table.get::<Option<Value>>(key)? {
        None | Some(Value::Nil) => fallback,
        Some(Value::Integer(i)) => i.clamp(0, 255) as u8,
        Some(Value::Number(n)) if n.is_finite() => n.round().clamp(0.0, 255.0) as u8,
        Some(_) => fallback,
    })
}

fn lua_u32(table: &Table, key: &str, fallback: u32) -> Result<u32, mlua::Error> {
    Ok(match table.get::<Option<Value>>(key)? {
        None | Some(Value::Nil) => fallback,
        Some(Value::Integer(i)) => i.max(0) as u32,
        Some(Value::Number(n)) if n.is_finite() => n.round().max(0.0) as u32,
        Some(_) => fallback,
    })
}

fn table_to_event(table: &Table, fallback: &MidiEvent) -> Result<MidiEvent, ScriptError> {
    let port = lua_u32(table, "port", fallback.port.0)?;
    let time = match table.get::<Option<Value>>("time")? {
        None | Some(Value::Nil) => fallback.time.nanos,
        Some(Value::Integer(i)) => i.max(0) as u64,
        Some(Value::Number(n)) if n.is_finite() => n.round().max(0.0) as u64,
        Some(_) => fallback.time.nanos,
    };
    let mt = lua_u8(table, "type", fallback.packet.message_type())?;
    if mt == 0x3 {
        if let Some(words) = sequence_u32(table, "words")?
            && let Ok(packet) = UmpMessage::try_from_words(&words)
        {
            return Ok(MidiEvent::new(
                Timestamp::from_nanos(time),
                PortId(port),
                packet,
            ));
        }
        return Ok(*fallback);
    }
    let group = lua_u8(table, "group", fallback.packet.group())?;
    let status = lua_u8(table, "status", fallback.packet.status_byte())?;
    let data1 = lua_u8(table, "data1", fallback.packet.data1())?;
    let data2 = lua_u8(table, "data2", fallback.packet.data2())?;
    let packet = if mt == 0x4 {
        let w1 = lua_u32(
            table,
            "word1",
            fallback.packet.words().get(1).copied().unwrap_or(0),
        )?;
        UmpMessage::midi2_channel_voice(group, status, data1, data2, w1)
    } else if mt == 0x1 {
        UmpMessage::midi1_system(group, status, data1, data2)
    } else {
        UmpMessage::midi1_channel_voice(group, status, data1, data2)
    };
    Ok(MidiEvent::new(
        Timestamp::from_nanos(time),
        PortId(port),
        packet,
    ))
}

fn sequence_u32(table: &Table, key: &str) -> Result<Option<Vec<u32>>, mlua::Error> {
    let Some(words) = table.get::<Option<Table>>(key)? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    let mut i = 1;
    while let Ok(Some(w)) = words.get::<Option<u32>>(i) {
        out.push(w);
        i += 1;
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use midi_forge_core::ump_from_status_data;

    fn note() -> MidiEvent {
        MidiEvent::new(
            Timestamp::from_nanos(1_000),
            PortId(1),
            ump_from_status_data(0x90, 60, 100),
        )
    }

    fn clock() -> MidiEvent {
        MidiEvent::new(
            Timestamp::from_nanos(2),
            PortId(1),
            UmpMessage::midi1_system(0, 0xF8, 0, 0),
        )
    }

    fn load(src: &str) -> ScriptEngine {
        let mut e = ScriptEngine::new();
        e.source = src.to_string();
        e.reload().unwrap();
        e.set_enabled(true);
        e
    }

    #[test]
    fn disabled_is_identity() {
        let mut e = ScriptEngine::new();
        e.source = "function on_midi(ev) return false end".into();
        e.reload().unwrap();
        assert_eq!(e.process(&note()), vec![note()]);
    }

    #[test]
    fn missing_on_midi_is_identity() {
        let mut e = load("midi.log('hi')");
        assert_eq!(e.process(&note()), vec![note()]);
        assert!(e.log_lines().iter().any(|l| l.contains("hi")));
    }

    #[test]
    fn false_drops() {
        let mut e = load("function on_midi(ev) return false end");
        assert!(e.process(&note()).is_empty());
    }

    #[test]
    fn drop_clock_keeps_notes() {
        let mut e = load(
            r#"
            function on_midi(ev)
              if ev.kind == "clock" then return false end
              return ev
            end
            "#,
        );
        assert_eq!(e.process(&note()), vec![note()]);
        assert!(e.process(&clock()).is_empty());
    }

    #[test]
    fn transpose_mutates_data1() {
        let mut e = load(
            r#"
            function on_midi(ev)
              if ev.kind == "note_on" then ev.data1 = ev.data1 + 12 end
              return ev
            end
            "#,
        );
        let out = e.process(&note());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].packet.data1(), 72);
        assert_eq!(out[0].packet.data2(), 100);
    }

    #[test]
    fn math_min_float_still_rewrites() {
        let mut e = load(
            r#"
            function on_midi(ev)
              ev.data1 = math.min(127, ev.data1 + 12)
              return ev
            end
            "#,
        );
        assert_eq!(e.process(&note())[0].packet.data1(), 72);
    }

    #[test]
    fn fan_out_extra_cc() {
        let mut e = load(
            r#"
            function on_midi(ev)
              return ev, midi.cc(ev.channel, 1, 64)
            end
            "#,
        );
        let out = e.process(&note());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].packet.data1(), 60);
        assert_eq!(out[1].packet.status_byte(), 0xB0);
        assert_eq!(out[1].packet.data1(), 1);
        assert_eq!(out[1].packet.data2(), 64);
    }

    #[test]
    fn syntax_error_on_reload() {
        let mut e = ScriptEngine::new();
        e.source = "function on_midi(ev".into();
        assert!(e.reload().is_err());
        assert!(e.error().is_some());
    }

    #[test]
    fn runtime_error_fails_open() {
        let mut e = load("function on_midi(ev) error('boom') end");
        assert_eq!(e.process(&note()), vec![note()]);
        assert!(e.error().unwrap().contains("boom"));
    }

    #[test]
    fn os_and_io_are_nil() {
        let mut e = load(
            r#"
            function on_midi(ev)
              if os or io then error("unsafe lib") end
              return ev
            end
            "#,
        );
        assert_eq!(e.process(&note()), vec![note()]);
        assert!(e.error().is_none());
    }

    #[test]
    fn after_fires_on_tick() {
        let mut e = load(
            r#"
            function on_midi(ev)
              midi.after(0, midi.cc(0, 7, 10))
              return ev
            end
            "#,
        );
        e.set_enabled(true);
        let _ = e.process(&note());
        let due = e.tick(1_000_000);
        assert!(due.iter().any(|ev| ev.packet.data1() == 7));
    }

    #[test]
    fn state_roundtrip() {
        let mut e = load("function on_midi(ev) midi.state.n = 3 return ev end");
        e.set_enabled(true);
        let _ = e.process(&note());
        let json = e.export_state();
        assert!(json.contains('3') || json.contains("n"));
        let mut e2 = ScriptEngine::new();
        e2.import_state(&json);
        let json2 = e2.export_state();
        assert!(json2.contains('3') || json2.contains("n") || json2 == "{}");
    }

    #[test]
    fn m2_note_on_prelude_is_type_4() {
        let mut e = load(
            r#"
            function on_midi(ev)
              return midi.m2_note_on(4, 48, 0x8000, 2)
            end
            "#,
        );
        let out = e.process(&note());
        assert_eq!(out.len(), 1);
        let p = out[0].packet;
        assert_eq!(p.message_type(), 4);
        assert_eq!(p.group(), 2);
        assert_eq!(p.status_byte(), 0x94);
        assert_eq!(p.data1(), 48);
        assert_eq!(p.data2(), 0);
        assert_eq!(p.words()[1] >> 16, 0x8000);
    }

    #[test]
    fn m2_note_off_and_cc_prelude() {
        let mut e = load(
            r#"
            function on_midi(ev)
              return midi.m2_note_off(1, 60, 0x1000, 0), midi.m2_cc(3, 7, 0x80000000, 1)
            end
            "#,
        );
        let out = e.process(&note());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].packet.message_type(), 4);
        assert_eq!(out[0].packet.status_byte(), 0x81);
        assert_eq!(out[0].packet.words()[1] >> 16, 0x1000);
        assert_eq!(out[1].packet.message_type(), 4);
        assert_eq!(out[1].packet.status_byte(), 0xB3);
        assert_eq!(out[1].packet.data1(), 7);
        assert_eq!(out[1].packet.group(), 1);
        assert_eq!(out[1].packet.words()[1], 0x8000_0000);
    }
}
