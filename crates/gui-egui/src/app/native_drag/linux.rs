//! Xdnd copy source for eframe's X11 window. No GTK window or event loop is needed.
use geezipx_egui::export::DeferredExport;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{ConnectionExt, *},
        Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
    CURRENT_TIME, NONE,
};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        XdndAware, XdndProxy, XdndEnter, XdndLeave, XdndPosition,
        XdndStatus, XdndDrop, XdndFinished, XdndSelection,
        XdndActionCopy, TARGETS, URI_LIST: b"text/uri-list",
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
struct Source {
    conn: RustConnection,
    root: Window,
    window: Window,
    atoms: Atoms,
    export: Arc<DeferredExport>,
    escape: Option<u8>,
}

pub(super) fn start(
    frame: &eframe::Frame,
    export: Arc<DeferredExport>,
    finished: impl Fn(std::result::Result<bool, String>) + Send + 'static,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::result::Result<(), String> {
    let display = frame.display_handle().map_err(|e| e.to_string())?;
    let RawDisplayHandle::Xlib(display) = display.as_raw() else {
        return Err(
            "当前 Wayland 会话尚不支持拖出，请使用 X11/XWayland 启动原生端，或使用解压操作。"
                .into(),
        );
    };
    let source = Source::new(export).map_err(|e| e.to_string())?;
    let xlib = x11_dl::xlib::Xlib::open().map_err(|e| e.to_string())?;
    let display_ptr = display.display.ok_or("X11 显示连接不可用")?;
    // SAFETY: Frame lends the live Xlib display and this runs on its UI thread.
    // Release winit's implicit button grab before our dedicated Xdnd connection
    // grabs the pointer. Never close or retain this borrowed display.
    unsafe {
        (xlib.XUngrabPointer)(display_ptr.as_ptr().cast(), CURRENT_TIME.into());
        (xlib.XFlush)(display_ptr.as_ptr().cast());
    }
    std::thread::Builder::new()
        .name("geezipx-xdnd".into())
        .spawn(move || {
            let result = source.run(&cancel).map_err(|e| e.to_string());
            finished(result);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

impl Source {
    fn new(export: Arc<DeferredExport>) -> Result<Self> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id()?;
        conn.create_window(
            0,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::default(),
        )?
        .check()?;
        let atoms = Atoms::new(&conn)?.reply()?;
        conn.set_selection_owner(window, atoms.XdndSelection, CURRENT_TIME)?
            .check()?;
        let setup = conn.setup();
        let mapping = conn
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        let escape = mapping
            .keysyms
            .chunks(mapping.keysyms_per_keycode.max(1) as usize)
            .position(|keys| keys.contains(&0xff1b))
            .map(|i| setup.min_keycode + i as u8);
        Ok(Self {
            conn,
            root,
            window,
            atoms,
            export,
            escape,
        })
    }
    fn send(&self, target: Window, kind: Atom, data: [u32; 5]) -> Result<()> {
        // A desktop may advertise a proxy window for protocol messages.
        let proxy = self
            .conn
            .get_property(false, target, self.atoms.XdndProxy, AtomEnum::WINDOW, 0, 1)?
            .reply()?
            .value32()
            .and_then(|mut v| v.next());
        let destination = if let Some(proxy) = proxy {
            let valid = self
                .conn
                .get_property(false, proxy, self.atoms.XdndProxy, AtomEnum::WINDOW, 0, 1)?
                .reply()?
                .value32()
                .and_then(|mut v| v.next());
            if valid == Some(proxy) {
                proxy
            } else {
                target
            }
        } else {
            target
        };
        self.conn.send_event(
            false,
            destination,
            EventMask::NO_EVENT,
            ClientMessageEvent::new(32, target, kind, data),
        )?;
        self.conn.flush()?;
        Ok(())
    }
    fn target(&self) -> Result<(Window, i16, i16, bool)> {
        let pointer = self.conn.query_pointer(self.root)?.reply()?;
        let mut window = pointer.child;
        let mut target = NONE;
        // Descend through window-manager frames and toolkit child windows.
        for _ in 0..32 {
            if window == NONE {
                break;
            }
            let version = self
                .conn
                .get_property(false, window, self.atoms.XdndAware, AtomEnum::ATOM, 0, 1)?
                .reply()?
                .value32()
                .and_then(|mut v| v.next());
            if version.is_some_and(|version| version >= 3) {
                target = window;
            }
            window = self.conn.query_pointer(window)?.reply()?.child;
        }
        Ok((
            target,
            pointer.root_x,
            pointer.root_y,
            pointer.mask.contains(KeyButMask::BUTTON1),
        ))
    }
    fn selection(&self, request: SelectionRequestEvent) -> Result<()> {
        let property = if request.property == NONE {
            request.target
        } else {
            request.property
        };
        let supported = if request.selection != self.atoms.XdndSelection {
            false
        } else if request.target == self.atoms.URI_LIST && self.export.is_dropped() {
            let mut uris = String::new();
            for path in self.export.paths()? {
                let uri = url::Url::from_file_path(&path).map_err(|()| "invalid drag path")?;
                uris.push_str(uri.as_str());
                uris.push_str("\r\n");
            }
            self.conn.change_property8(
                PropMode::REPLACE,
                request.requestor,
                property,
                self.atoms.URI_LIST,
                uris.as_bytes(),
            )?;
            true
        } else if request.target == self.atoms.TARGETS {
            self.conn.change_property32(
                PropMode::REPLACE,
                request.requestor,
                property,
                AtomEnum::ATOM,
                &[self.atoms.URI_LIST, self.atoms.TARGETS],
            )?;
            true
        } else {
            false
        };
        self.conn.send_event(
            false,
            request.requestor,
            EventMask::NO_EVENT,
            SelectionNotifyEvent {
                response_type: SELECTION_NOTIFY_EVENT,
                sequence: 0,
                time: request.time,
                requestor: request.requestor,
                selection: request.selection,
                target: request.target,
                property: if supported { property } else { NONE },
            },
        )?;
        self.conn.flush()?;
        Ok(())
    }
    fn run(self, cancel: &std::sync::atomic::AtomicBool) -> Result<bool> {
        let grab = self
            .conn
            .grab_pointer(
                false,
                self.root,
                EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                NONE,
                NONE,
                CURRENT_TIME,
            )?
            .reply()?;
        if grab.status != GrabStatus::SUCCESS {
            return Err("无法接管拖动指针".into());
        }
        let result = self.track(cancel);
        let _ = self.conn.ungrab_pointer(CURRENT_TIME);
        let _ = self.conn.destroy_window(self.window);
        let _ = self.conn.flush();
        result
    }
    fn track(&self, cancel: &std::sync::atomic::AtomicBool) -> Result<bool> {
        let mut target = NONE;
        let mut accepted = false;
        let mut last_position = None;
        let mut dropping = None;
        let started = Instant::now();
        loop {
            while let Some(event) = self.conn.poll_for_event()? {
                match event {
                    Event::SelectionRequest(request) => self.selection(request)?,
                    Event::SelectionClear(_) => return Ok(false),
                    Event::ClientMessage(event) if event.type_ == self.atoms.XdndStatus => {
                        let data = event.data.as_data32();
                        if data[0] == target {
                            accepted = data[1] & 1 != 0 && data[4] == self.atoms.XdndActionCopy;
                        }
                    }
                    Event::ClientMessage(event) if event.type_ == self.atoms.XdndFinished => {
                        let data = event.data.as_data32();
                        if dropping.is_some() && data[0] == target {
                            return Ok(data[1] & 1 != 0);
                        }
                    }
                    _ => {}
                }
            }
            if let Some(time) = dropping {
                if Instant::now().duration_since(time) > Duration::from_secs(30) {
                    return Err("接收方未确认拖放完成".into());
                }
            } else {
                let escaped = if let Some(key) = self.escape {
                    self.conn.query_keymap()?.reply()?.keys[key as usize / 8] & (1 << (key % 8))
                        != 0
                } else {
                    false
                };
                if escaped
                    || cancel.load(std::sync::atomic::Ordering::Relaxed)
                    || started.elapsed() > Duration::from_secs(300)
                {
                    if target != NONE {
                        self.send(target, self.atoms.XdndLeave, [self.window, 0, 0, 0, 0])?;
                    }
                    return Ok(false);
                }
                let (next, x, y, down) = self.target()?;
                if next != target {
                    if target != NONE {
                        self.send(target, self.atoms.XdndLeave, [self.window, 0, 0, 0, 0])?;
                    }
                    target = next;
                    accepted = false;
                    last_position = None;
                    if target != NONE {
                        self.send(
                            target,
                            self.atoms.XdndEnter,
                            [self.window, 5 << 24, self.atoms.URI_LIST, 0, 0],
                        )?;
                    }
                }
                if target != NONE && last_position != Some((x, y)) {
                    self.send(
                        target,
                        self.atoms.XdndPosition,
                        [
                            self.window,
                            0,
                            ((x as u16 as u32) << 16) | y as u16 as u32,
                            CURRENT_TIME,
                            self.atoms.XdndActionCopy,
                        ],
                    )?;
                    last_position = Some((x, y));
                }
                if !down {
                    if target != NONE && accepted {
                        self.export.confirm_drop();
                        self.send(
                            target,
                            self.atoms.XdndDrop,
                            [self.window, 0, CURRENT_TIME, 0, 0],
                        )?;
                        self.conn.ungrab_pointer(CURRENT_TIME)?;
                        self.conn.flush()?;
                        dropping = Some(Instant::now());
                    } else {
                        if target != NONE {
                            self.send(target, self.atoms.XdndLeave, [self.window, 0, 0, 0, 0])?;
                        }
                        return Ok(false);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
