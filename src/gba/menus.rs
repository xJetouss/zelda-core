//! Menüs im GBA-Modus (Teil der Menüleiste aus `shell.rs`, deshalb hier als dessen Untermodul):
//! Datei, Spielstand, Emulation, Grafik, Audio, Steuerung und Cheats – dieselben Emulator-Funktionen wie bei
//! A Link to the Past, nur ohne Mods, Recompiled-Modus und Spielfunktionen (Karte, Inventar …).

use super::*;

/// Fertige Cheats für „The Minish Cap“ (Europa BZMP / USA BZME). Spielstand im RAM ab 0x02002A40 (Aufbau aus der
/// Dekompilierung zeldaret/tmc); alle Codes im Spiel nachgeprüft (Anzeige bzw. Item-Menü). Items, Fähigkeiten und
/// Flaschen per ODER-Code: sie setzen nur Bits und löschen nichts, was das Spiel selbst umschaltet.
pub(super) const MINISH_EU: &[(&str, &str, &str)] = &[
    ("20 Herzen, unendlich", "20 hearts, infinite", "32002AEB 00A0
32002AEA 00A0"),
    ("10 Herzen, unendlich", "10 hearts, infinite", "32002AEB 0050
32002AEA 0050"),
    ("999 Rubine (größte Geldbörse)", "999 rupees (biggest wallet)", "32002AE8 0003
82002B00 03E7"),
    ("999 Muscheln", "999 shells", "82002B02 03E7"),
    ("99 Bomben (größte Tasche)", "99 bombs (biggest bag)", "32002AEE 0003
32002AEC 0063"),
    ("99 Pfeile (größter Köcher)", "99 arrows (biggest quiver)", "32002AEF 0003
32002AED 0063"),
    ("Alle Items und 4 Flaschen", "All items and 4 bottles", "22002B32 5554
22002B34 5555
22002B36 5554
22002B38 5500
22002B42 1500
22002AF6 2020
22002AF8 2020"),
    ("Alle Schwerttechniken", "All sword techniques", "22002B44 5555"),
];

/// Spielkennung der laufenden GBA-ROM (z. B. „BZMP“)
fn game_code(r: &Root) -> String {
    r.app.gba.game.as_ref().and_then(|g| std::fs::read(&g.path).ok()).and_then(|b| crate::gba::header(&b)).map(|h| h.1).unwrap_or_default()
}

impl Root {
    /// Menüpunkte im GBA-Modus (gleiche Nummern wie die Menüleiste)
    pub(super) fn gba_menu_items(&mut self, i: usize, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let s = self.app.s.clone();
        let mut v: Vec<AnyElement> = Vec::new();
        macro_rules! chk {
            ($id:expr, $txt:expr, $($f:tt)+) => {{
                let val = self.app.$($f)+;
                v.push(check($id, $txt, val, true, cx, |r| r.app.$($f)+ = !r.app.$($f)+).into_any_element());
            }};
        }
        match i {
            0 => {
                v.push(self.item("gba_open", t("ROM öffnen…", "Open ROM…"), self.hk("open"), true, false, cx, |r| r.app.open_dialog()));
                v.push(sep().into_any_element());
                v.push(self.item("gba_reset", "Reset", self.hk("reset"), true, false, cx, |r| {
                    if let Some(g) = r.app.gba.game.as_mut() {
                        g.reset();
                    }
                }));
                v.push(self.item("gba_menu", t("Zurück zum Hauptmenü", "Back to main menu"), String::new(), true, false, cx, |r| r.app.go_main_menu()));
                v.push(Self::note(t("Der Spielstand des Spiels (.sav) wird automatisch neben der ROM gespeichert.", "The in-game save (.sav) is written next to the ROM automatically.")));
                v.push(sep().into_any_element());
                v.push(self.item("gba_quit", t("Beenden", "Quit"), self.hk("quit"), true, false, cx, |r| r.gctx.send_viewport_cmd(gfx::ViewportCommand::Close)));
                if let Some(g) = &self.app.gba.game {
                    let info = std::fs::read(&g.path).ok().and_then(|b| crate::gba::header(&b)).map(|(t, c)| format!("{t} · {c} · {:.2} FPS", g.fps)).unwrap_or_default();
                    v.push(sep().into_any_element());
                    v.push(div().px(px(8.0)).py(px(1.0)).text_size(px(11.0)).text_color(col::faint()).child(format!("Game Boy Advance · mGBA · {info}")).into_any_element());
                }
            }
            1 => {
                v.push(self.item("gba_states", t("Spielstände… (Home)", "Save states… (Home)"), self.hk("win_states"), true, true, cx, |r| r.app.win_states = true));
                v.push(sep().into_any_element());
                for slot in 1..=9u8 {
                    let when = self.app.gba_slot_time(slot);
                    let label = format!("{} {slot}", t("Platz", "Slot"));
                    let info = when.clone().unwrap_or_else(|| t("leer", "empty").to_string());
                    let r = row()
                        .px(px(8.0))
                        .min_h(px(30.0))
                        .child(div().w(px(70.0)).child(label))
                        .child(div().flex_1().text_size(px(12.0)).text_color(col::dim()).child(info))
                        .child(button(format!("gba-save-{slot}"), t("Speichern", "Save"), true, cx, move |r| r.app.save_slot(slot)))
                        .child(button(format!("gba-load-{slot}"), t("Laden", "Load"), when.is_some(), cx, move |r| r.app.load_slot(slot)));
                    v.push(r.into_any_element());
                }
                v.push(Self::note(t("Spielstände liegen als .gs1 … .gs9 neben der ROM.", "Save states are stored as .gs1 … .gs9 next to the ROM.")));
            }
            2 => {
                let k = self.hk("pause");
                let paused = self.app.paused;
                v.push(check("gba_pause", format!("{}   [{k}]", t("Pause", "Pause")), paused, true, cx, |r| r.app.paused = !r.app.paused).into_any_element());
                v.push(self.item("gba_adv", t("Einzelbild vor", "Frame advance"), self.hk("frame"), paused, false, cx, |r| {
                    r.app.advance = true;
                    r.ui.open_menu = Some(2);
                }));
                v.push(sep().into_any_element());
                v.push(Self::heading(t("Geschwindigkeit", "Speed")));
                let mut sp = wrap_row().px(px(2.0));
                for (val, n) in [(0.5f32, "50%"), (1.0, "100%"), (1.5, "150%"), (2.0, "200%"), (3.0, "300%")] {
                    sp = sp.child(radio(format!("gba-speed-{n}"), n, s.speed == val, cx, move |r| r.app.s.speed = val));
                }
                v.push(sp.into_any_element());
                v.push(Self::note(format!("{} {} · {} {}", self.hk("double"), t("halten = ×2", "hold = ×2"), self.hk("turbo"), t("halten = Turbo", "hold = turbo"))));
                let ts = s.turbo_speed as f64;
                v.push(slider(self, "gba_turbo", "Turbo", ts, 2.0, 10.0, 0.0, format!("{ts:.1}×"), true, cx, |r, x| r.app.s.turbo_speed = x as f32).into_any_element());
                v.push(sep().into_any_element());
                let k = self.hk("rewind");
                let rw = s.rewind;
                v.push(check("gba_rewind", format!("{} [{k}]", t("Zurückspulen", "Rewind")), rw, true, cx, |r| r.app.s.rewind = !r.app.s.rewind).into_any_element());
                let rs = s.rewind_seconds as f64;
                v.push(slider(self, "gba_rewind_s", t("Sekunden", "seconds"), rs, 5.0, 120.0, 1.0, format!("{}", rs as u32), rw, cx, |r, x| r.app.s.rewind_seconds = x.round() as _).into_any_element());
                chk!("gba_pause_menus", t("Pause, solange ein Fenster offen ist", "Pause while a window is open"), s.pause_in_menus);
                chk!("gba_auto_pause", t("Automatisch pausieren, wenn das Fenster nicht aktiv ist", "Pause automatically when the window is not active"), s.auto_pause);
            }
            3 => {
                v.push(Self::heading(t("Bild / Upscaler", "Image / upscaler")));
                for f in Filter::GBA_ALL {
                    let sel = s.filter == f;
                    v.push(
                        radio(format!("gba-filter-{}", f as u8), f.name(en), sel, cx, move |r| {
                            r.app.s.filter = f;
                            if f.is_hd() {
                                r.app.s.last_hd = f;
                            }
                        })
                        .into_any_element(),
                    );
                }
                v.push(sep().into_any_element());
                v.push(Self::heading(t("Breitbild", "Widescreen")));
                let mut ws = wrap_row().px(px(2.0));
                for (val, n) in [(0u8, "3:2"), (1, "16:9"), (2, t("21:9 Ultrawide", "21:9 ultrawide")), (3, t("Fenster füllen", "Fill window"))] {
                    ws = ws.child(radio(format!("gba-ws-{val}"), n, s.widescreen == val, cx, move |r| r.app.s.widescreen = val));
                }
                v.push(ws.into_any_element());
                if crate::gba::picori::available() {
                    let rc = s.gba_rc;
                    v.push(
                        check("gba_rc", t("Recomp (Picori): The Minish Cap als PC-Fassung mit echtem Breitbild", "Recomp (Picori): The Minish Cap as PC port with true widescreen"), rc, true, cx, |r| {
                            r.app.s.gba_rc = !r.app.s.gba_rc;
                            // neu starten, damit der andere Kern übernimmt (Spielstand des Spiels bleibt, Savestates je Kern)
                            if let Some(p) = r.app.gba.game.as_ref().map(|g| g.path.clone()) {
                                r.app.gba_open(p);
                            }
                        })
                        .into_any_element(),
                    );
                    v.push(Self::note(t("Mit Recomp zeigt 16:9/21:9 wirklich mehr von der Spielwelt (bis 384 Pixel breit). Savestates von mGBA und Recomp sind getrennt.", "With recomp, 16:9/21:9 really shows more of the world (up to 384 pixels wide). mGBA and recomp save states are separate.")));
                }
                chk!("gba_stretch", t("Bild auf die Fläche strecken (sonst Rand aus dem Spielbild)", "Stretch the image (otherwise a border from the game image)"), s.gba_stretch);
                if s.widescreen != 0 && !s.gba_stretch {
                    v.push(Self::note(t("Das GBA-Bild selbst ist 3:2. Links und rechts erscheint das Spielbild weich und dunkel als Rand.", "The GBA image itself is 3:2. The game image fills the sides, soft and dark.")));
                }
                v.push(sep().into_any_element());
                v.push(Self::heading(t("Bildrate", "Frame rate")));
                let mut fr = wrap_row().px(px(2.0));
                for (val, n) in [(0u32, t("Monitor (VSync)", "Monitor (VSync)")), (120, "120"), (144, "144"), (200, "200"), (240, "240"), (360, "360"), (500, "500"), (1000, t("Unbegrenzt (1000)", "Unlimited (1000)"))] {
                    fr = fr.child(radio(format!("gba-fps-{val}"), n, s.fps_limit == val, cx, move |r| r.app.s.fps_limit = val));
                }
                v.push(fr.into_any_element());
                chk!("gba_blend", t("Überblenden (weichere Bewegung über 60 FPS)", "Frame blending (smoother motion above 60 FPS)"), s.frame_blend);
                chk!("gba_fps", t("FPS anzeigen", "Show FPS"), s.show_fps);
                v.push(sep().into_any_element());
                chk!("gba_integer", t("Ganzzahlige Skalierung", "Integer scaling"), s.integer_scale);
                let sat = s.saturation as f64;
                v.push(slider(self, "gba_sat", t("Sättigung", "Saturation"), sat, 0.0, 2.0, 0.0, format!("{sat:.2}"), true, cx, |r, x| r.app.s.saturation = x as f32).into_any_element());
                let gam = s.gamma as f64;
                v.push(slider(self, "gba_gamma", "Gamma", gam, 0.5, 2.0, 0.0, format!("{gam:.2}"), true, cx, |r, x| r.app.s.gamma = x as f32).into_any_element());
                chk!("gba_color_rando", t("Farb-Randomizer", "Color randomizer"), s.color_rando);
                v.push(sep().into_any_element());
                v.push(self.item("gba_full", t("Vollbild", "Fullscreen"), self.hk("fullscreen"), true, false, cx, |r| {
                    let c = r.gctx.clone();
                    r.app.toggle_fullscreen(&c);
                }));
                v.push(self.item("gba_hidebar", t("Menüleiste ausblenden", "Hide menu bar"), self.hk("menu"), true, false, cx, |r| r.app.show_bar = false));
            }
            4 => {
                let k = self.hk("mute");
                let m = s.mute;
                v.push(check("gba_mute", format!("{} [{k}]", t("Stumm", "Mute")), m, true, cx, |r| r.app.s.mute = !r.app.s.mute).into_any_element());
                let vol = s.volume as f64;
                v.push(slider(self, "gba_vol", t("Lautstärke", "Volume"), vol, 0.0, 1.5, 0.0, format!("{:.0}%", vol * 100.0), true, cx, |r, x| r.app.s.volume = x as f32).into_any_element());
                let lat = s.latency_ms as f64;
                v.push(slider(self, "gba_lat", t("Latenz", "Latency"), lat, 30.0, 250.0, 1.0, format!("{} ms", lat as u32), true, cx, |r, x| r.app.s.latency_ms = x.round() as _).into_any_element());
                chk!("gba_mute_turbo", t("Im Turbo stumm", "Mute while fast-forwarding"), s.mute_turbo);
                if let Some(a) = &self.app.audio {
                    v.push(div().px(px(8.0)).py(px(1.0)).text_size(px(11.0)).text_color(col::faint()).child(format!("Audio: {} · {} Hz", a.device_name, a.device_rate)).into_any_element());
                }
            }
            5 => {
                v.push(self.item("gba_keys", t("Tasten belegen…", "Map buttons…"), self.hk("win_controls"), true, false, cx, |r| r.app.win_controls = true));
                v.push(self.item("gba_hotkeys", t("Hotkeys belegen…", "Map hotkeys…"), self.hk("win_hotkeys"), true, false, cx, |r| r.app.win_hotkeys = true));
                v.push(sep().into_any_element());
                chk!("gba_pad_label", t("Controller: A/B nach Beschriftung (Xbox)", "Controller: A/B by label (Xbox)"), s.pad_label_layout);
                let dz = s.deadzone as f64;
                v.push(slider(self, "gba_deadzone", t("Stick-Totzone", "Stick dead zone"), dz, 0.1, 0.9, 0.0, format!("{dz:.2}"), true, cx, |r, x| r.app.s.deadzone = x as f32).into_any_element());
                chk!("gba_block_opp", t("Gegenrichtungen: zuletzt gedrückte gewinnt", "Opposite directions: last pressed wins"), s.block_opposite);
                v.push(Self::note(t("GBA-Tasten: A, B, L, R, Start, Select und Steuerkreuz wie in der Tastenbelegung. Home / Switch-Home öffnet die Spielstände mit Einstellungen und Hauptmenü.", "GBA buttons: A, B, L, R, Start, Select and d-pad as mapped. Home opens the save states with settings and main menu.")));
            }
            6 => {
                // Spiel: Zusatzfunktionen für The Minish Cap an/aus
                v.push(Self::heading(t("The Minish Cap", "The Minish Cap")));
                chk!("gba_y", t("Y-Taste (zusätzliches Item auf Y)", "Y button (extra item on Y)"), s.gba_y);
                chk!("gba_bar", t("Item-Leiste über das Steuerkreuz (↓ B, → A, ← Y)", "Item bar on the d-pad (↓ B, → A, ← Y)"), s.gba_bar);
                chk!("gba_map", t("Weltkarte von Anfang an (Item-Menü: L)", "World map from the start (item menu: L)"), s.gba_map);
                chk!("gba_ownmap", t("Eigene Karte mit 100 % (Minus)", "Own map with 100 % (Minus)"), s.gba_ownmap);
                chk!("gba_mapwin", t("Karte im eigenen Fenster", "Map in its own window"), s.map_own_window);
            }
            7 => {
                v.push(self.item("gba_cheats", t("Cheat-Liste…", "Cheat list…"), String::new(), true, true, cx, |r| r.app.gba.win_cheats = true));
                if matches!(game_code(self).as_str(), "BZMP" | "BZME") {
                    v.push(Self::heading(t("Fertige Cheats", "Ready-made cheats")));
                    for (k, (de, e, code)) in MINISH_EU.iter().enumerate() {
                        let on = self.app.gba.game.as_ref().is_some_and(|g| g.cfg.cheats.iter().any(|c| c.code == *code && c.on));
                        v.push(check(format!("gba-preset-{k}"), if en { *e } else { *de }, on, true, cx, move |r| gba_toggle_preset(r, k)).into_any_element());
                    }
                }
                let cheats = self.app.gba.game.as_ref().map(|g| g.cfg.cheats.clone()).unwrap_or_default();
                let own: Vec<(usize, crate::gba::Cheat)> = cheats.into_iter().enumerate().filter(|(_, c)| !MINISH_EU.iter().any(|p| p.2 == c.code)).collect();
                if !own.is_empty() {
                    v.push(Self::heading(t("Eigene Cheats", "Own cheats")));
                }
                for (k, c) in own {
                    v.push(check(format!("gba-cheat-{k}"), c.name.clone(), c.on, true, cx, move |r| gba_toggle_cheat(r, k)).into_any_element());
                }
            }
            _ => {}
        }
        v
    }

    /// Fenster „Cheat-Liste“ (fertige Cheats, eigene Codes: GameShark, Action Replay, CodeBreaker, Adresse:Wert)
    pub(super) fn gba_cheats_ui(&mut self, cx: &mut Context<Root>) -> Vec<AnyElement> {
        let en = self.app.s.ui_en;
        let t = move |d: &'static str, e: &'static str| tr(en, d, e);
        let mut v = Vec::new();
        if matches!(game_code(self).as_str(), "BZMP" | "BZME") {
            // wie bei A Link to the Past: Abschnitte, Schalter in zwei Spalten, Werte direkt einstellbar
            let preset = |r: &mut Root, k: usize, cx: &mut Context<Root>| {
                let (de, e, code) = MINISH_EU[k];
                let on = r.app.gba.game.as_ref().is_some_and(|g| g.cfg.cheats.iter().any(|c| c.code == code && c.on));
                switch(format!("gba-pw-{k}"), if en { e } else { de }, on, true, cx, move |r| gba_toggle_preset(r, k)).into_any_element()
            };
            // Spielwerte ab 0x02002AE8 (Geldbörse, Herzen, Bomben, Pfeile, A/B, Rubine, Muscheln)
            let mem = self.app.gba.game.as_ref().and_then(|g| g.peek(0x0200_2AE8, 0x1C));
            let has = mem.is_some();
            let tode = self.app.gba.deaths;
            v.push(
                section(
                    t("Tode", "Deaths"),
                    vec![
                        small(format!("{}: {tode}", t("Tode im laufenden Spiel", "Deaths in the current game"))).into_any_element(),
                        row()
                            .pt(px(4.0))
                            .gap(px(8.0))
                            .child(button("gba-d-reset", t("Tode zurücksetzen", "Reset deaths"), true, cx, |r| r.app.reset_deaths(false)))
                            .child(button("gba-d-reset-all", t("… auch in allen Spielständen", "… also in all save states"), true, cx, |r| r.app.reset_deaths(true)))
                            .into_any_element(),
                    ],
                )
                .into_any_element(),
            );
            let player = vec![preset(self, 0, cx), preset(self, 1, cx), preset(self, 7, cx)];
            let mut pl = vec![grid(2, player, 12.0).into_any_element()];
            if let Some(m) = &mem {
                let cap = (m[3] / 8).clamp(1, 20) as f64;
                let cur = m[2] as f64 / 8.0;
                let line = |txt: &str, s: Div| row().child(div().w(px(200.0)).flex_none().child(txt.to_string())).child(s).into_any_element();
                let s1 = slider(self, "gba-hearts-cap", "", cap, 1.0, 20.0, 1.0, format!("{}", cap as u32), true, cx, |r, x| {
                    if let Some(g) = r.app.gba.game.as_mut() {
                        let cap = (x.round() as u32 * 8) as u8;
                        let hp = g.peek(0x0200_2AEA, 1).map_or(cap, |b| b[0].min(cap));
                        g.poke(0x0200_2AEA, &[hp, cap]);
                    }
                });
                pl.push(line(t("Herzcontainer", "Heart containers"), s1));
                let s2 = slider(self, "gba-hearts-cur", "", cur, 0.25, cap, 0.25, format!("{cur:.2}"), true, cx, |r, x| {
                    if let Some(g) = r.app.gba.game.as_mut() {
                        let cap = g.peek(0x0200_2AEB, 1).map_or(24, |b| b[0]) as u32;
                        g.poke(0x0200_2AEA, &[((x * 8.0).round() as u32).clamp(2, cap.max(2)) as u8]);
                    }
                });
                pl.push(line(t("Aktuelle Herzen", "Current hearts"), s2));
            }
            v.push(section(t("Spieler", "Player"), pl).into_any_element());
            let inv = vec![preset(self, 2, cx), preset(self, 3, cx), preset(self, 4, cx), preset(self, 5, cx), preset(self, 6, cx)];
            let mut ib = vec![grid(2, inv, 12.0).into_any_element()];
            ib.push(
                row()
                    .pt(px(4.0))
                    .gap(px(8.0))
                    .child(button("gba-give-all", t("Alle Items geben", "Give all items"), has, cx, move |r| {
                        if let Some(g) = r.app.gba.game.as_mut() {
                            g.run_once(MINISH_EU[6].2);
                        }
                        r.app.msg(tr(en, "Alle Items erhalten", "All items given"));
                    }))
                    .child(button("gba-give-tech", t("Alle Schwerttechniken", "All sword techniques"), has, cx, move |r| {
                        if let Some(g) = r.app.gba.game.as_mut() {
                            g.run_once(MINISH_EU[7].2);
                        }
                        r.app.msg(tr(en, "Alle Schwerttechniken erhalten", "All sword techniques given"));
                    }))
                    .child(button("gba-give-20", t("20 Herzcontainer", "20 heart containers"), has, cx, move |r| {
                        if let Some(g) = r.app.gba.game.as_mut() {
                            g.poke(0x0200_2AEA, &[0xA0, 0xA0]);
                        }
                        r.app.msg(tr(en, "20 Herzen", "20 hearts"));
                    }))
                    .into_any_element(),
            );
            if let Some(m) = &mem {
                let cell = |txt: &str, d: Div| row().justify_between().child(div().text_color(col::dim()).child(txt.to_string())).child(d).into_any_element();
                let u16at = |o: usize| u16::from_le_bytes([m[o], m[o + 1]]) as f64;
                let w16 = |addr: u32| {
                    move |r: &mut Root, x: f64| {
                        if let Some(g) = r.app.gba.game.as_mut() {
                            g.poke(addr, &(x.round().clamp(0.0, 999.0) as u16).to_le_bytes());
                        }
                    }
                };
                let w8 = |addr: u32| {
                    move |r: &mut Root, x: f64| {
                        if let Some(g) = r.app.gba.game.as_mut() {
                            g.poke(addr, &[x.round().clamp(0.0, 99.0) as u8]);
                        }
                    }
                };
                let counts = vec![
                    cell(t("Rubine", "Rupees"), drag_value("gba-dv-rupees", u16at(0x18), 0.0, 999.0, true, cx, w16(0x0200_2B00))),
                    cell(t("Muscheln", "Shells"), drag_value("gba-dv-shells", u16at(0x1A), 0.0, 999.0, true, cx, w16(0x0200_2B02))),
                    cell(t("Bomben", "Bombs"), drag_value("gba-dv-bombs", m[4] as f64, 0.0, 99.0, true, cx, w8(0x0200_2AEC))),
                    cell(t("Pfeile", "Arrows"), drag_value("gba-dv-arrows", m[5] as f64, 0.0, 99.0, true, cx, w8(0x0200_2AED))),
                ];
                ib.push(sep().into_any_element());
                ib.push(grid(2, counts, 18.0).into_any_element());
            }
            v.push(section(t("Inventar", "Inventory"), ib).into_any_element());
            // Ausrüstung: Items auf A, B und Y (wie die Item-Leiste im Spiel: Steuerkreuz → / ↓ / ←)
            let eq = match (&mem, self.app.gba.game.as_ref()) {
                (Some(m), Some(g)) => Some((m[0xC], m[0xD], g.cfg.y_item, g.owned_items())),
                _ => None,
            };
            if let Some((a_cur, b_cur, y_cur, owned)) = eq {
                let mut row_eq = wrap_row();
                for (k, (lbl, cur)) in [("A", a_cur), ("B", b_cur), ("Y", y_cur)].into_iter().enumerate() {
                    let mut list: Vec<u8> = vec![0];
                    list.extend(owned.iter().copied());
                    let name = move |i: u8| if i == 0 { tr(en, "Nichts", "Nothing").to_string() } else { crate::gba::minish::name(i, en).to_string() };
                    let opts: Vec<(String, bool)> = list.iter().map(|&i| (name(i), i == cur)).collect();
                    let cb = combo(self, &format!("gba-eq-{k}"), name(cur), 170.0, opts, cx, move |r, i| {
                        let item = list[i];
                        if let Some(g) = r.app.gba.game.as_mut() {
                            match k {
                                0 => g.equip(true, item),
                                1 => g.equip(false, item),
                                _ => {
                                    g.cfg.y_item = item;
                                    g.save_cfg();
                                }
                            }
                        }
                    });
                    row_eq = row_eq.child(row().gap(px(6.0)).child(strong(lbl)).child(cb));
                }
                v.push(
                    section(
                        t("Ausrüstung", "Equipment"),
                        vec![
                            row_eq.into_any_element(),
                            small(t(
                                "Im Spiel am Controller: Steuerkreuz ↓ halten = Item für B, → = für A, ← = für Y. Mit Stick, L/R oder ←/→ wählen, loslassen = ausrüsten. Kurz antippen: Leiste bleibt offen, A wählt, B bricht ab.",
                                "In game on a controller: hold d-pad ↓ = item for B, → = for A, ← = for Y. Pick with the stick, L/R or ←/→, release to equip. Tap briefly: the bar stays open, A picks, B cancels.",
                            ))
                            .into_any_element(),
                        ],
                    )
                    .into_any_element(),
                );
            }
        }
        let cheats = self.app.gba.game.as_ref().map(|g| g.cfg.cheats.clone()).unwrap_or_default();
        let mut rows = Vec::new();
        let own: Vec<(usize, crate::gba::Cheat)> = cheats.into_iter().enumerate().filter(|(_, c)| !MINISH_EU.iter().any(|p| p.2 == c.code)).collect();
        if own.is_empty() {
            rows.push(small(t("Noch keine eigenen Cheats. Unten Name und Code eintragen und „Hinzufügen“ drücken.", "No own cheats yet. Enter a name and a code below and press \"Add\".")).into_any_element());
        }
        for (k, c) in own {
            let text = div().flex().flex_col().gap(px(2.0)).child(strong(c.name.clone())).child(div().font_family(MONO_FONT).text_size(px(11.0)).text_color(col::dim()).child(c.code.replace('\n', "  +  ")));
            let sw = switch(format!("gba-cw-{k}"), text, c.on, true, cx, move |r| gba_toggle_cheat(r, k));
            let del = button(format!("gba-cdel-{k}"), t("Löschen", "Delete"), true, cx, move |r| {
                if let Some(g) = r.app.gba.game.as_mut() {
                    if k < g.cfg.cheats.len() {
                        g.cfg.cheats.remove(k);
                        g.save_cfg();
                        g.apply_cheats();
                    }
                }
            });
            rows.push(row().gap(px(10.0)).child(div().flex_1().min_w(px(0.0)).child(sw)).child(del).into_any_element());
        }
        v.push(section(t("Eigene Cheats und Codes", "Own cheats and codes"), rows).into_any_element());
        let name = self.app.gba.cheat_name.clone();
        let code = self.app.gba.cheat_code.clone();
        let add = vec![
            small(t("Codes für GameShark, Action Replay, CodeBreaker oder „Adresse:Wert“ (VBA), passend zur Version deiner ROM. Mehrere Zeilen mit „+“ trennen.", "Codes for GameShark, Action Replay, CodeBreaker or \"address:value\" (VBA), matching your ROM version. Separate several lines with \"+\".")).into_any_element(),
            row().gap(px(8.0)).child(text_field(self, TextField::GbaCheatName, &name, t("Name, z. B. Unendlich Bomben", "Name, e.g. infinite bombs"), 230.0, cx)).into_any_element(),
            row()
                .gap(px(8.0))
                .child(text_field(self, TextField::GbaCheatCode, &code, "XXXXXXXX YYYYYYYY + …", 330.0, cx))
                .child(button("gba-cadd", t("Hinzufügen", "Add"), !code.trim().is_empty(), cx, |r| {
                    let code: String = r.app.gba.cheat_code.split(['+', ';']).map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
                    if code.is_empty() {
                        return;
                    }
                    let name = if r.app.gba.cheat_name.trim().is_empty() { format!("Cheat {}", r.app.gba.game.as_ref().map_or(0, |g| g.cfg.cheats.len()) + 1) } else { r.app.gba.cheat_name.trim().to_string() };
                    if let Some(g) = r.app.gba.game.as_mut() {
                        g.cfg.cheats.push(crate::gba::Cheat { name, code, on: true });
                        g.save_cfg();
                        g.apply_cheats();
                    }
                    r.app.gba.cheat_name.clear();
                    r.app.gba.cheat_code.clear();
                }))
                .into_any_element(),
        ];
        v.push(section(t("Neuer Cheat", "New cheat"), add).into_any_element());
        v.push(small(t("Cheats gelten sofort und werden je Spiel neben der ROM gespeichert (.gba.json).", "Cheats apply immediately and are stored per game next to the ROM (.gba.json).")).into_any_element());
        v
    }
}

fn gba_toggle_cheat(r: &mut Root, k: usize) {
    if let Some(g) = r.app.gba.game.as_mut() {
        if let Some(c) = g.cfg.cheats.get_mut(k) {
            c.on = !c.on;
        }
        g.save_cfg();
        g.apply_cheats();
    }
}

/// fertigen Cheat an/aus (wird dazu in die Cheat-Liste des Spiels übernommen)
fn gba_toggle_preset(r: &mut Root, k: usize) {
    let Some(&(de, _, code)) = MINISH_EU.get(k) else { return };
    if let Some(g) = r.app.gba.game.as_mut() {
        match g.cfg.cheats.iter_mut().find(|c| c.code == code) {
            Some(c) => c.on = !c.on,
            None => g.cfg.cheats.push(crate::gba::Cheat { name: de.to_string(), code: code.to_string(), on: true }),
        }
        // Herz-Cheats schließen sich gegenseitig aus
        if code.starts_with("32002AEB") {
            let on = g.cfg.cheats.iter().any(|c| c.code == code && c.on);
            if on {
                for c in g.cfg.cheats.iter_mut().filter(|c| c.code != code && c.code.starts_with("32002AEB")) {
                    c.on = false;
                }
            }
        }
        g.save_cfg();
        g.apply_cheats();
    }
}
