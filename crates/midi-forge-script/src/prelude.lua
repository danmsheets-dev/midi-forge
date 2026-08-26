-- Helpers injected before user scripts. No io/os/package.
midi = midi or {}

local function chn(ch)
  return math.floor(tonumber(ch) or 0) % 16
end

function midi.note_on(ch, note, vel, group)
  ch = chn(ch)
  return {
    type = 2,
    group = group or 0,
    status = 0x90 + ch,
    channel = ch,
    data1 = note,
    data2 = vel,
    kind = "note_on",
  }
end

function midi.note_off(ch, note, vel, group)
  ch = chn(ch)
  return {
    type = 2,
    group = group or 0,
    status = 0x80 + ch,
    channel = ch,
    data1 = note,
    data2 = vel or 0,
    kind = "note_off",
  }
end

function midi.cc(ch, cc, val, group)
  ch = chn(ch)
  return {
    type = 2,
    group = group or 0,
    status = 0xB0 + ch,
    channel = ch,
    data1 = cc,
    data2 = val,
    kind = "cc",
  }
end

function midi.pitch_bend(ch, lsb, msb, group)
  ch = chn(ch)
  return {
    type = 2,
    group = group or 0,
    status = 0xE0 + ch,
    channel = ch,
    data1 = lsb,
    data2 = msb,
    kind = "pitch_bend",
  }
end
