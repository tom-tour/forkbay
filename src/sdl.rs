// Controllers, read through SDL3 and set up as the emulators set it up (their
// src/input_common/drivers/sdl_driver.cpp, the same in Eden and its forks). NX2 Launcher then sees the
// same controllers, with the same ids, ports and button numbers, and the controls it writes work
// in every emulator.
// SDL lives on a thread of its own, started on first use and stopped after a minute unused.
use crate::params::{float, Params};
use anyhow::{anyhow, Result};
use sdl3_sys::events::{SDL_FlushEvents, SDL_PumpEvents};
use sdl3_sys::gamepad::*;
use sdl3_sys::hints::*;
use sdl3_sys::init::{SDL_Init, SDL_Quit, SDL_INIT_GAMEPAD, SDL_INIT_JOYSTICK};
use sdl3_sys::joystick::*;
use sdl3_sys::sensor::{SDL_SENSOR_ACCEL, SDL_SENSOR_GYRO};
use sdl3_sys::stdinc::SDL_free;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

/// The emulators' settings that change which controllers SDL handles and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub raw_input: bool,
    pub disable_wgi_xinput: bool,
    pub joycon_driver: bool,
    pub procon_driver: bool,
}

/// One controller, at the port the emulators give it: its rank among those with the same id.
/// A controller unplugged keeps its port for the next one with that id, as in the emulators.
struct Pad {
    id: Option<SDL_JoystickID>,
    joy: *mut SDL_Joystick,
    gamepad: *mut SDL_Gamepad,
    guid: String,
    port: usize,
    name: String,
    motion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Button,
    Stick,
    Motion,
}

#[derive(Clone, Default)]
struct Snapshot {
    buttons: Vec<bool>,
    axes: Vec<f32>,
    hats: Vec<u8>,
    gyro: [f32; 3],
}

/// Waiting for a control, as the emulators do: the state when it started is kept, and the first
/// difference is the answer.
struct Capture {
    kind: Kind,
    only: Option<(String, usize)>,
    start: Vec<(String, usize, Snapshot)>,
    axes: Vec<i32>,
    result: Option<String>,
}

pub struct Sdl {
    pads: Vec<Pad>,
    capture: Option<Capture>,
}

fn guid_text(guid: SDL_GUID) -> String {
    let mut data = guid.data;
    // The emulators clear the name check, so that a renamed controller keeps its controls.
    data[2] = 0;
    data[3] = 0;
    data.iter().map(|b| format!("{b:02x}")).collect()
}

use sdl3_sys::guid::SDL_GUID;

fn text(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

fn bytes(guid: &str) -> Vec<u8> {
    (0..guid.len() / 2).filter_map(|i| u8::from_str_radix(&guid[i * 2..i * 2 + 2], 16).ok()).collect()
}

/// Joy-Con and Pro Controllers are left to the emulators' own driver when it is on.
fn left_to_own_driver(guid: &str, o: &Options) -> bool {
    let b = bytes(guid);
    if b.len() < 9 || b[4] != 0x7e || b[5] != 0x05 { return false; }
    (o.joycon_driver && (b[8] == 0x06 || b[8] == 0x07)) || (o.procon_driver && b[8] == 0x09)
}

fn hat_name(mask: u8) -> &'static str {
    match mask {
        SDL_HAT_UP => "up",
        SDL_HAT_DOWN => "down",
        SDL_HAT_LEFT => "left",
        SDL_HAT_RIGHT => "right",
        _ => "",
    }
}

impl Pad {
    fn connected(&self) -> bool {
        !self.joy.is_null()
    }

    fn snapshot(&self) -> Snapshot {
        if !self.connected() { return Snapshot::default(); }
        unsafe {
            let j = self.joy;
            let mut gyro = [0f32; 3];
            if !self.gamepad.is_null() && self.motion {
                SDL_GetGamepadSensorData(self.gamepad, SDL_SENSOR_GYRO, gyro.as_mut_ptr(), 3);
            }
            Snapshot {
                buttons: (0..SDL_GetNumJoystickButtons(j)).map(|i| SDL_GetJoystickButton(j, i)).collect(),
                axes: (0..SDL_GetNumJoystickAxes(j)).map(|i| SDL_GetJoystickAxis(j, i) as f32 / 32767.0).collect(),
                hats: (0..SDL_GetNumJoystickHats(j)).map(|i| SDL_GetJoystickHat(j, i)).collect(),
                gyro,
            }
        }
    }

    fn name_now(&self) -> String {
        unsafe {
            if !self.gamepad.is_null() {
                let fixed = match SDL_GetGamepadType(self.gamepad) {
                    SDL_GAMEPAD_TYPE_XBOX360 => Some("Xbox 360 Controller"),
                    SDL_GAMEPAD_TYPE_XBOXONE => Some("Xbox One Controller"),
                    SDL_GAMEPAD_TYPE_PS3 => Some("DualShock 3 Controller"),
                    SDL_GAMEPAD_TYPE_PS4 => Some("DualShock 4 Controller"),
                    SDL_GAMEPAD_TYPE_PS5 => Some("DualSense Controller"),
                    _ => None,
                };
                if let Some(n) = fixed { return n.to_string(); }
                if let Some(n) = text(SDL_GetGamepadName(self.gamepad)) { return n; }
            }
            text(SDL_GetJoystickName(self.joy)).unwrap_or_else(|| "Unknown".into())
        }
    }

    fn is_joycon_left(&self) -> bool {
        self.name.contains("Joy-Con Left") || self.name.contains("Joy-Con (L)")
    }

    fn is_joycon_right(&self) -> bool {
        self.name.contains("Joy-Con Right") || self.name.contains("Joy-Con (R)")
    }

    fn base(&self) -> Params {
        Params::new(&[("engine", "sdl".into()), ("port", self.port.to_string()), ("guid", self.guid.clone())])
    }

    fn bindings(&self) -> Vec<SDL_GamepadBinding> {
        if self.gamepad.is_null() { return vec![]; }
        unsafe {
            let mut count = 0;
            let list = SDL_GetGamepadBindings(self.gamepad, &mut count);
            if list.is_null() { return vec![]; }
            let out = (0..count as usize).filter_map(|i| { let b = *list.add(i); (!b.is_null()).then(|| *b) }).collect();
            SDL_free(list.cast());
            out
        }
    }

    fn axis_now(&self, axis: i32) -> f32 {
        if self.joy.is_null() { return 0.0; }
        unsafe { SDL_GetJoystickAxis(self.joy, axis) as f32 / 32767.0 }
    }
}

/// The parameters for one binding, as BuildParamPackageForBinding writes them.
fn for_binding(pad: &Pad, b: &SDL_GamepadBinding) -> Params {
    let mut p = pad.base();
    unsafe {
        match b.input_type {
            SDL_GAMEPAD_BINDTYPE_AXIS => {
                p.set("axis", b.input.axis.axis);
                p.set("threshold", "0.5");
                p.set("invert", "+");
            }
            SDL_GAMEPAD_BINDTYPE_BUTTON => p.set("button", b.input.button),
            SDL_GAMEPAD_BINDTYPE_HAT => {
                p.set("hat", b.input.hat.hat);
                p.set("direction", hat_name(b.input.hat.hat_mask as u8));
            }
            _ => return Params::default(),
        }
    }
    p
}

fn for_button(bindings: &[SDL_GamepadBinding], button: SDL_GamepadButton) -> SDL_GamepadBinding {
    bindings.iter().copied().find(|b| b.output_type == SDL_GAMEPAD_BINDTYPE_BUTTON && unsafe { b.output.button } == button).unwrap_or_default()
}

fn for_axis(bindings: &[SDL_GamepadBinding], axis: SDL_GamepadAxis) -> SDL_GamepadBinding {
    bindings.iter().copied().find(|b| b.output_type == SDL_GAMEPAD_BINDTYPE_AXIS && unsafe { b.output.axis.axis } == axis).unwrap_or_default()
}

/// The Switch buttons, in the emulators' order, with the SDL button each one is given by default.
fn default_buttons(pad: &Pad) -> Vec<(&'static str, SDL_GamepadButton)> {
    let (mut sll, mut srl, mut slr, mut srr) = (SDL_GAMEPAD_BUTTON_LEFT_SHOULDER, SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER, SDL_GAMEPAD_BUTTON_LEFT_SHOULDER, SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER);
    if pad.is_joycon_left() { sll = SDL_GAMEPAD_BUTTON_LEFT_PADDLE1; srl = SDL_GAMEPAD_BUTTON_LEFT_PADDLE2; }
    if pad.is_joycon_right() { slr = SDL_GAMEPAD_BUTTON_RIGHT_PADDLE2; srr = SDL_GAMEPAD_BUTTON_RIGHT_PADDLE1; }
    vec![
        ("button_a", SDL_GAMEPAD_BUTTON_EAST),
        ("button_b", SDL_GAMEPAD_BUTTON_SOUTH),
        ("button_x", SDL_GAMEPAD_BUTTON_NORTH),
        ("button_y", SDL_GAMEPAD_BUTTON_WEST),
        ("button_lstick", SDL_GAMEPAD_BUTTON_LEFT_STICK),
        ("button_rstick", SDL_GAMEPAD_BUTTON_RIGHT_STICK),
        ("button_l", SDL_GAMEPAD_BUTTON_LEFT_SHOULDER),
        ("button_r", SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER),
        ("button_plus", SDL_GAMEPAD_BUTTON_START),
        ("button_minus", SDL_GAMEPAD_BUTTON_BACK),
        ("button_dleft", SDL_GAMEPAD_BUTTON_DPAD_LEFT),
        ("button_dup", SDL_GAMEPAD_BUTTON_DPAD_UP),
        ("button_dright", SDL_GAMEPAD_BUTTON_DPAD_RIGHT),
        ("button_ddown", SDL_GAMEPAD_BUTTON_DPAD_DOWN),
        ("button_slleft", sll),
        ("button_srleft", srl),
        ("button_slright", slr),
        ("button_srright", srr),
        ("button_home", SDL_GAMEPAD_BUTTON_GUIDE),
        ("button_screenshot", SDL_GAMEPAD_BUTTON_MISC1),
    ]
}

const Z_AXES: [(&str, SDL_GamepadAxis); 2] = [("button_zl", SDL_GAMEPAD_AXIS_LEFT_TRIGGER), ("button_zr", SDL_GAMEPAD_AXIS_RIGHT_TRIGGER)];

/// Buttons of the left Joy-Con, taken from the second controller of a pair.
fn on_left_side(name: &str) -> bool {
    matches!(name, "button_ddown" | "button_dleft" | "button_dright" | "button_dup" | "button_l" | "button_lstick" | "button_minus" | "button_screenshot" | "button_zl")
}

impl Sdl {
    fn start(o: &Options) -> Result<Self> {
        let set = |name: *const c_char, value: &CStr| unsafe { SDL_SetHint(name, value.as_ptr()); };
        let on_off = |b: bool| if b { c"1" } else { c"0" };
        unsafe {
            set(SDL_HINT_APP_NAME, c"NX2 Launcher");
            set(SDL_HINT_JOYSTICK_RAWINPUT, on_off(o.raw_input));
            if cfg!(windows) && o.disable_wgi_xinput {
                SDL_SetHintWithPriority(SDL_HINT_JOYSTICK_RAWINPUT_CORRELATE_XINPUT, c"0".as_ptr(), SDL_HINT_OVERRIDE);
                SDL_SetHintWithPriority(SDL_HINT_JOYSTICK_WGI, c"0".as_ptr(), SDL_HINT_OVERRIDE);
            }
            set(SDL_HINT_JOYSTICK_HIDAPI_STEAM, c"1");
            set(SDL_HINT_GAMECONTROLLER_SENSOR_FUSION, c"1");
            set(SDL_HINT_AUTO_UPDATE_SENSORS, c"1");
            set(SDL_HINT_JOYSTICK_ENHANCED_REPORTS, c"1");
            set(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, c"1");
            if o.joycon_driver {
                set(SDL_HINT_JOYSTICK_HIDAPI_JOY_CONS, c"0");
            } else {
                set(SDL_HINT_JOYSTICK_HIDAPI_JOY_CONS, c"1");
                set(SDL_HINT_JOYSTICK_HIDAPI_JOYCON_HOME_LED, c"0");
                set(SDL_HINT_JOYSTICK_HIDAPI_COMBINE_JOY_CONS, c"0");
                set(SDL_HINT_JOYSTICK_HIDAPI_VERTICAL_JOY_CONS, c"1");
            }
            if o.procon_driver {
                set(SDL_HINT_JOYSTICK_HIDAPI_SWITCH, c"0");
            } else {
                set(SDL_HINT_JOYSTICK_HIDAPI_SWITCH, c"1");
                set(SDL_HINT_JOYSTICK_HIDAPI_SWITCH_HOME_LED, c"0");
            }
            set(SDL_HINT_JOYSTICK_HIDAPI_SWITCH_PLAYER_LED, c"1");
            set(SDL_HINT_JOYSTICK_HIDAPI_XBOX, c"0");
            if !SDL_Init(SDL_INIT_JOYSTICK | SDL_INIT_GAMEPAD) {
                return Err(anyhow!("SDL : {}", text(sdl3_sys::error::SDL_GetError()).unwrap_or_default()));
            }
        }
        let mut s = Self { pads: vec![], capture: None };
        // SDL finds some controllers in the background, just after it starts.
        let until = Instant::now() + Duration::from_millis(400);
        while Instant::now() < until {
            s.refresh(o);
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(s)
    }

    /// Opens the controllers plugged in since the last look and lets go of those unplugged.
    fn refresh(&mut self, o: &Options) {
        unsafe {
            SDL_UpdateJoysticks();
            let mut count = 0;
            let list = SDL_GetJoysticks(&mut count);
            let ids: Vec<SDL_JoystickID> = if list.is_null() { vec![] } else { let v = (0..count as usize).map(|i| *list.add(i)).collect(); SDL_free(list.cast()); v };
            for p in self.pads.iter_mut().filter(|p| p.id.is_some_and(|id| !ids.contains(&id))) {
                if !p.gamepad.is_null() { SDL_CloseGamepad(p.gamepad); } else if !p.joy.is_null() { SDL_CloseJoystick(p.joy); }
                p.joy = std::ptr::null_mut();
                p.gamepad = std::ptr::null_mut();
                p.id = None;
            }
            for id in ids {
                if self.pads.iter().any(|p| p.id == Some(id)) { continue; }
                let gamepad = if SDL_IsGamepad(id) { SDL_OpenGamepad(id) } else { std::ptr::null_mut() };
                let joy = if gamepad.is_null() { SDL_OpenJoystick(id) } else { SDL_GetGamepadJoystick(gamepad) };
                if joy.is_null() { continue; }
                let guid = guid_text(SDL_GetJoystickGUID(joy));
                if left_to_own_driver(&guid, o) {
                    if !gamepad.is_null() { SDL_CloseGamepad(gamepad); } else { SDL_CloseJoystick(joy); }
                    // Remembered as handled, so that it is not opened again on every look.
                    self.pads.push(Pad { id: Some(id), joy: std::ptr::null_mut(), gamepad: std::ptr::null_mut(), guid: String::new(), port: usize::MAX, name: String::new(), motion: false });
                    continue;
                }
                let mut motion = false;
                if !gamepad.is_null() {
                    let accel = SDL_GamepadHasSensor(gamepad, SDL_SENSOR_ACCEL);
                    let gyro = SDL_GamepadHasSensor(gamepad, SDL_SENSOR_GYRO);
                    if accel { SDL_SetGamepadSensorEnabled(gamepad, SDL_SENSOR_ACCEL, true); }
                    if gyro { SDL_SetGamepadSensorEnabled(gamepad, SDL_SENSOR_GYRO, true); }
                    motion = accel || gyro;
                }
                let free_slot = self.pads.iter().position(|p| p.guid == guid && !p.connected() && p.port != usize::MAX);
                let mut pad = Pad { id: Some(id), joy, gamepad, guid: guid.clone(), port: 0, name: String::new(), motion };
                pad.name = pad.name_now();
                match free_slot {
                    Some(i) => { pad.port = self.pads[i].port; self.pads[i] = pad; }
                    None => { pad.port = self.pads.iter().filter(|p| p.guid == guid).count(); self.pads.push(pad); }
                }
            }
            SDL_PumpEvents();
            SDL_FlushEvents(0, 0xFFFF);
        }
    }

    fn live(&self) -> impl Iterator<Item = &Pad> {
        self.pads.iter().filter(|p| p.connected())
    }

    fn find(&self, guid: &str, port: usize) -> Option<&Pad> {
        self.live().find(|p| p.guid == guid && p.port == port)
    }

    /// The controllers the emulators would list, with Joy-Con pairs as one more choice.
    pub fn devices(&self) -> Value {
        let mut out: Vec<Value> = self.live().map(|p| json!({ "name": format!("{} {}", p.name, p.port), "guid": p.guid, "port": p.port, "motion": p.motion, "gamepad": !p.gamepad.is_null() })).collect();
        for right in self.live().filter(|p| p.is_joycon_right()) {
            if let Some(left) = self.live().find(|p| p.is_joycon_left() && p.port == right.port) {
                out.push(json!({ "name": format!("Nintendo Dual Joy-Con {}", right.port), "guid": right.guid, "guid2": left.guid, "port": right.port, "motion": right.motion, "gamepad": true }));
            }
        }
        json!(out)
    }

    /// What each controller is doing now, for the page to light up the controls it uses.
    pub fn states(&self) -> Value {
        json!(self.live().map(|p| { let s = p.snapshot(); json!({ "guid": p.guid, "port": p.port, "buttons": s.buttons, "axes": s.axes, "hats": s.hats }) }).collect::<Vec<_>>())
    }

    /// The controls a controller gets when it is chosen, as GetButtonMappingForDevice,
    /// GetAnalogMappingForDevice and GetMotionMappingForDevice build them.
    pub fn defaults(&self, guid: &str, port: usize, guid2: Option<&str>) -> Result<Value> {
        let pad = self.find(guid, port).ok_or_else(|| anyhow!("Cette manette n'est plus branchée."))?;
        let pad2 = guid2.and_then(|g| self.find(g, port)).filter(|p| !p.gamepad.is_null());
        let mut buttons = serde_json::Map::new();
        let mut sticks = serde_json::Map::new();
        let mut motions = serde_json::Map::new();
        let list = default_buttons(pad);
        if pad.gamepad.is_null() {
            // A controller SDL cannot map: its own numbers are used as they are.
            for (name, b) in &list {
                let mut p = pad.base();
                p.set("button", b.0);
                buttons.insert(name.to_string(), json!(p.serialize()));
            }
            for (name, a) in Z_AXES {
                let mut p = pad.base();
                p.set("axis", a.0);
                p.set("threshold", "0.5");
                p.set("invert", "+");
                buttons.insert(name.to_string(), json!(p.serialize()));
            }
            return Ok(json!({ "buttons": buttons, "sticks": sticks, "motions": motions }));
        }
        let b1 = pad.bindings();
        let b2 = pad2.map(Pad::bindings).unwrap_or_default();
        for (name, b) in &list {
            let (p, bs) = match pad2 { Some(p2) if on_left_side(name) => (p2, &b2), _ => (pad, &b1) };
            buttons.insert(name.to_string(), json!(for_binding(p, &for_button(bs, *b)).serialize()));
        }
        for (name, a) in Z_AXES {
            let (p, bs) = match pad2 { Some(p2) if on_left_side(name) => (p2, &b2), _ => (pad, &b1) };
            buttons.insert(name.to_string(), json!(for_binding(p, &for_axis(bs, a)).serialize()));
        }
        let stick = |p: &Pad, bs: &[SDL_GamepadBinding], x: SDL_GamepadAxis, y: SDL_GamepadAxis| {
            let (ax, ay) = unsafe { (for_axis(bs, x).input.axis.axis, for_axis(bs, y).input.axis.axis) };
            let mut s = p.base();
            s.set("axis_x", ax);
            s.set("axis_y", ay);
            s.set("offset_x", float(-p.axis_now(ax)));
            s.set("offset_y", float(p.axis_now(ay)));
            s.set("invert_x", "+");
            s.set("invert_y", "+");
            s.serialize()
        };
        let (lp, lb) = match pad2 { Some(p2) => (p2, &b2), None => (pad, &b1) };
        sticks.insert("lstick".into(), json!(stick(lp, lb, SDL_GAMEPAD_AXIS_LEFTX, SDL_GAMEPAD_AXIS_LEFTY)));
        sticks.insert("rstick".into(), json!(stick(pad, &b1, SDL_GAMEPAD_AXIS_RIGHTX, SDL_GAMEPAD_AXIS_RIGHTY)));
        let motion = |p: &Pad| { let mut m = p.base(); m.set("motion", 0); m.serialize() };
        if pad.motion { motions.insert("motionright".into(), json!(motion(pad))); }
        match pad2 {
            Some(p2) if p2.motion => { motions.insert("motionleft".into(), json!(motion(p2))); }
            None if pad.motion => { motions.insert("motionleft".into(), json!(motion(pad))); }
            _ => {}
        }
        Ok(json!({ "buttons": buttons, "sticks": sticks, "motions": motions }))
    }

    pub fn begin(&mut self, kind: Kind, only: Option<(String, usize)>) {
        let start = self.live().map(|p| (p.guid.clone(), p.port, p.snapshot())).collect();
        self.capture = Some(Capture { kind, only, start, axes: vec![], result: None });
    }

    pub fn stop(&mut self) {
        self.capture = None;
    }

    /// The control found since `begin`, if any, as the emulators' MappingFactory builds it.
    pub fn poll(&mut self) -> Option<String> {
        let mut cap = self.capture.take()?;
        if cap.result.is_none() {
            'pads: for p in self.live() {
                if cap.only.as_ref().is_some_and(|(g, port)| (g != &p.guid) || *port != p.port) { continue; }
                let Some((_, _, before)) = cap.start.iter().find(|(g, port, _)| g == &p.guid && *port == p.port) else { continue };
                let now = p.snapshot();
                let mut base = p.base();
                base.set("pad", 0);
                for (i, (a, b)) in now.buttons.iter().zip(&before.buttons).enumerate() {
                    if a != b {
                        base.set("button", i);
                        cap.result = Some(base.serialize());
                        break 'pads;
                    }
                }
                for (i, (a, b)) in now.hats.iter().zip(&before.hats).enumerate() {
                    let changed = a ^ b;
                    if let Some(bit) = [SDL_HAT_UP, SDL_HAT_RIGHT, SDL_HAT_DOWN, SDL_HAT_LEFT].into_iter().find(|m| changed & m != 0) {
                        base.set("hat", i);
                        base.set("direction", hat_name(bit));
                        cap.result = Some(base.serialize());
                        break 'pads;
                    }
                }
                for (i, (a, b)) in now.axes.iter().zip(&before.axes).enumerate() {
                    if (a - b).abs() < 0.5 { continue; }
                    let i = i as i32;
                    match cap.kind {
                        Kind::Button => {
                            base.set("axis", i);
                            base.set("threshold", float(0.5));
                            cap.result = Some(base.serialize());
                            break 'pads;
                        }
                        Kind::Stick | Kind::Motion => {
                            if cap.axes.contains(&i) { continue; }
                            cap.axes.push(i);
                            let need = if cap.kind == Kind::Stick { 2 } else { 3 };
                            if cap.axes.len() < need { continue; }
                            base.set("axis_x", cap.axes[0]);
                            base.set("axis_y", cap.axes[1]);
                            if cap.kind == Kind::Stick {
                                base.set("threshold", float(0.5));
                                base.set("range", float(0.95));
                                base.set("deadzone", float(0.15));
                            } else {
                                base.set("axis_z", cap.axes[2]);
                                base.set("range", float(1.0));
                                base.set("deadzone", float(0.2));
                            }
                            cap.result = Some(base.serialize());
                            break 'pads;
                        }
                    }
                }
                if cap.kind == Kind::Motion && p.motion && now.gyro.iter().zip(before.gyro).any(|(a, b)| (a - b).abs() > 0.6 * std::f32::consts::TAU) {
                    base.set("motion", 0);
                    cap.result = Some(base.serialize());
                    break 'pads;
                }
            }
        }
        let found = cap.result.clone();
        if found.is_none() { self.capture = Some(cap); }
        found
    }

    /// The controllers plugged in, for the controls to follow them before a game starts.
    pub fn plugged(&self) -> Vec<(String, usize)> {
        self.live().map(|p| (p.guid.clone(), p.port)).collect()
    }

    fn close(&mut self) {
        unsafe {
            for p in self.pads.iter().filter(|p| p.connected()) {
                if !p.gamepad.is_null() { SDL_CloseGamepad(p.gamepad); } else { SDL_CloseJoystick(p.joy); }
            }
            self.pads.clear();
            SDL_Quit();
        }
    }
}

type Job = Box<dyn FnOnce(&mut Sdl) + Send>;
static WORKER: Mutex<Option<mpsc::Sender<Job>>> = Mutex::new(None);

fn spawn(o: Options) -> Result<mpsc::Sender<Job>> {
    let (tx, rx) = mpsc::channel::<Job>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    std::thread::spawn(move || {
        let mut sdl = match Sdl::start(&o) {
            Ok(s) => { let _ = ready_tx.send(Ok(())); s }
            Err(e) => { let _ = ready_tx.send(Err(e.to_string())); return; }
        };
        let mut last = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(15)) {
                Ok(job) => { sdl.refresh(&o); job(&mut sdl); last = Instant::now(); }
                Err(mpsc::RecvTimeoutError::Timeout) => sdl.refresh(&o),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if last.elapsed() > Duration::from_secs(60) {
                // Stopped under the lock, so that no request is left waiting for this thread.
                let mut w = WORKER.lock().unwrap_or_else(|e| e.into_inner());
                while let Ok(job) = rx.try_recv() { job(&mut sdl); }
                *w = None;
                break;
            }
        }
        sdl.close();
    });
    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(tx),
        Ok(Err(e)) => Err(anyhow!(e)),
        Err(_) => Err(anyhow!("SDL ne répond pas.")),
    }
}

/// Lets go of the controllers now, for a game about to start.
pub fn stop() {
    WORKER.lock().unwrap_or_else(|e| e.into_inner()).take();
}

/// Runs `f` on the SDL thread, started with these settings if it is not running.
pub fn call<T: Send + 'static>(o: Options, f: impl FnOnce(&mut Sdl) -> T + Send + 'static) -> Result<T> {
    let (tx, rx) = mpsc::channel();
    let mut job: Job = Box::new(move |s| { let _ = tx.send(f(s)); });
    {
        let mut w = WORKER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = w.as_ref() {
            match s.send(job) {
                Ok(()) => { drop(w); return rx.recv_timeout(Duration::from_secs(10)).map_err(|_| anyhow!("SDL ne répond pas.")); }
                Err(e) => job = e.0,
            }
        }
        let s = spawn(o)?;
        s.send(job).map_err(|_| anyhow!("SDL ne répond pas."))?;
        *w = Some(s);
    }
    rx.recv_timeout(Duration::from_secs(10)).map_err(|_| anyhow!("SDL ne répond pas."))
}
