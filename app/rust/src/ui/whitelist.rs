//! Owned by WP-14: whitelist editing window (`WhitelistDevicesForm.cs`, dark theme per AD-31).

use super::controls::{ButtonSpec, Ctl, LabelSpec, ListSpec};
use super::layout::{Anchor, FlowDir, Node, Track};
use super::msgbox::{self, Answer, Buttons, Icon};
use super::theme;
use super::window::{self, Event, FormSpec, FormStyle, WindowSize};
use crate::clean::{self, devices::GhostDevice, whitelist as store};
use std::cell::Cell;
use std::rc::Rc;
use windows::Win32::Foundation::HWND;

const TITLE: &str = "Manage Device Whitelist";
const LIST: u16 = 1;
const CANCEL: u16 = 2;
const SAVE: u16 = 3;
const RESET: u16 = 4;

/// Shows the whitelist editor and returns true only after a successful save.
pub fn show(owner: HWND, devices: &[GhostDevice]) -> bool {
    // C# parity: WhitelistDevicesForm.cs:129. C# reads a bad file as empty without a word;
    // here the error is shown once and every item starts unchecked.
    let whitelisted = match store::load_whitelist() {
        Ok(list) => list,
        Err(error) => {
            msgbox::show(owner, &error, TITLE, Buttons::Ok, Icon::Warning);
            Vec::new()
        }
    };
    // C# parity: WhitelistDevicesForm.cs:131-141. Scan order, `{Description} ({Class})`.
    let items: Vec<(String, bool)> = devices
        .iter()
        .map(|d| {
            (
                format!("{} ({})", d.description, d.class),
                store::is_whitelisted(d, &whitelisted),
            )
        })
        .collect();
    let devices = devices.to_vec();
    let saved = Rc::new(Cell::new(false));
    let s = Rc::clone(&saved);
    let mut spec = FormSpec::new(TITLE, WindowSize::Client(theme::WHITELIST_CLIENT_SIZE));
    spec.min = Some(theme::WHITELIST_MIN_SIZE);
    spec.style = FormStyle::Sizable {
        maximize: true,
        minimize: false,
    };
    spec.accept = Some(SAVE);
    spec.cancel = Some(CANCEL);
    let shown = window::run_modal(owner, spec, tree(), move |form, event| {
        let handled = crate::win::catch_panic(|| match event {
            Event::Created => form.list_set_items(LIST, &items),
            Event::Click(SAVE) => {
                // C# parity: WhitelistDevicesForm.cs:144-158. Only the checked rows of this
                // scan are kept; every other entry of the file is dropped.
                let keep: Vec<GhostDevice> = form
                    .list_checked(LIST)
                    .iter()
                    .zip(&devices)
                    .filter(|(checked, _)| **checked)
                    .map(|(_, d)| d.clone())
                    .collect();
                match store::save_whitelist(&keep) {
                    Ok(()) => {
                        s.set(true);
                        form.destroy();
                    }
                    Err(error) => report_failure(form.hwnd(), "Save device whitelist", &error),
                }
            }
            Event::Click(RESET) => {
                let answer = msgbox::show(
                    form.hwnd(),
                    "Are you sure you want to reset the whitelist? This will remove all whitelisted devices.",
                    "Confirm Reset",
                    Buttons::YesNo,
                    Icon::Warning,
                );
                if answer == Answer::Yes {
                    // C# parity: WhitelistDevicesForm.cs:168-172. Deleted at once; Cancel
                    // does not undo it.
                    match store::reset_whitelist() {
                        Ok(()) => {
                            for i in 0..devices.len() {
                                form.list_set_checked(LIST, i, false);
                            }
                        }
                        Err(error) => report_failure(form.hwnd(), "Reset device whitelist", &error),
                    }
                }
            }
            Event::Click(CANCEL) => form.destroy(),
            _ => {}
        });
        if let Err(panic) = handled {
            msgbox::show(form.hwnd(), &panic, "Error", Buttons::Ok, Icon::Error);
        }
        true
    });
    if let Err(error) = shown {
        msgbox::show(owner, &error.to_string(), "Error", Buttons::Ok, Icon::Error);
        return false;
    }
    saved.get()
}

/// A whitelist write that did not happen: never shown as success, the window stays open.
fn report_failure(owner: HWND, what: &str, error: &str) {
    // The guard decides; a no-op probe asks it without touching the file again.
    if clean::destructive(what, || ()).is_none() {
        msgbox::show(
            owner,
            &format!("[DRY RUN] {what}"),
            TITLE,
            Buttons::Ok,
            Icon::Information,
        );
    } else {
        // C# lets this exception escape to the WinForms crash dialog (no designed text).
        msgbox::show(owner, error, "Error", Buttons::Ok, Icon::Error);
    }
}

fn tree() -> Vec<Node> {
    let action = |id: u16, text: &str| {
        // C# parity: WhitelistDevicesForm.cs:109-124. ApplyStyle overrides the 12,4 padding.
        // DESIGN.md 6: Save is the main action, Reset deletes the whitelist file.
        let spec = match id {
            SAVE => ButtonSpec::primary(text),
            RESET => ButtonSpec::destructive(text),
            _ => ButtonSpec::outline(text),
        };
        Node::leaf(id, Ctl::Button(spec))
            .auto_size()
            .min(theme::ACTION_BUTTON_MIN)
            .padding(theme::SHARED_BUTTON_PADDING)
            .margin(theme::ACTION_BUTTON_MARGIN)
    };
    let list = ListSpec {
        font: theme::WHITELIST_LIST_FONT,
        fore: theme::TEXT_BOX_TEXT,
        back: theme::TEXT_BOX_BACKGROUND,
        selected_back: theme::LIST_SELECTED_BACKGROUND,
        selected_fore: theme::LIST_SELECTED_TEXT,
    };
    vec![
        Node::table(
            vec![Track::Percent(100.0)],
            vec![
                Track::AutoSize,
                Track::Percent(100.0),
                Track::Absolute(theme::ACTION_ROW_HEIGHT),
            ],
            vec![
                Node::leaf(
                    0,
                    Ctl::Label(LabelSpec::new(
                        "Select ghost devices to keep in the whitelist:",
                        theme::WHITELIST_HEADER_FONT,
                        theme::PRIMARY_TEXT,
                    )),
                )
                .auto_size()
                .anchor(Anchor::LEFT)
                .margin(theme::WHITELIST_HEADER_MARGIN)
                .cell(0, 0),
                Node::panel(vec![Node::leaf(LIST, Ctl::CheckedList(list)).fill()])
                    .fill()
                    .padding(theme::OUTPUT_PANEL_PADDING)
                    .cell(0, 1),
                // Add order Cancel, Save, Reset: visual Reset / Save / Cancel, tab order as added.
                Node::flow(
                    FlowDir::RightToLeft,
                    false,
                    vec![
                        action(CANCEL, "Cancel"),
                        action(SAVE, "Save Whitelist"),
                        action(RESET, "Reset Whitelist"),
                    ],
                )
                .fill()
                .padding(theme::ACTION_PANEL_PADDING)
                .back(theme::BUTTON_PANEL_BACKGROUND)
                .cell(0, 2),
            ],
        )
        .fill(),
    ]
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::super::{controls, window::Form};
    use super::*;
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            LB_GETCURSEL, LB_GETTEXT, LB_GETTEXTLEN, LB_GETTOPINDEX, LB_SETCURSEL, LB_SETTOPINDEX,
            SendMessageW, WM_CHAR, WM_LBUTTONDOWN, WM_LBUTTONUP,
        },
    };

    fn message(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
        // SAFETY: Synchronous messages to the live test list; pointer arguments below
        // reference buffers that remain alive for the complete call.
        unsafe { SendMessageW(hwnd, msg, Some(WPARAM(wparam)), Some(LPARAM(lparam))).0 }
    }

    fn native_name(hwnd: HWND, index: usize) -> String {
        let len = message(hwnd, LB_GETTEXTLEN, index, 0);
        assert!(len >= 0);
        let mut text = vec![0u16; len as usize + 1];
        assert_eq!(
            message(hwnd, LB_GETTEXT, index, text.as_mut_ptr() as isize),
            len
        );
        crate::win::wide::from_wide(&text)
    }

    #[test]
    #[ignore = "opens a fixture whitelist; reads native accessible names without saving"]
    fn checked_list_native_accessibility() {
        assert!(super::super::dpi::set_per_monitor_v2_for_tests());
        let form = Form::create(
            HWND::default(),
            FormSpec::new(TITLE, WindowSize::Client(theme::WHITELIST_CLIENT_SIZE)),
            tree(),
            |_, _| true,
        )
        .expect("fixture whitelist");
        form.show();
        let list = form.control(LIST).expect("checked list");
        let items: Vec<_> = (0..80)
            .map(|i| (format!("Fixture {i} (USB), checked"), i == 1))
            .collect();
        form.list_set_items(LIST, &items);
        assert_eq!(native_name(list, 0), "Fixture 0 (USB), checked, unchecked");
        assert_eq!(native_name(list, 1), "Fixture 1 (USB), checked, checked");
        message(list, LB_SETCURSEL, 0, 0);
        for checked in [true, false] {
            message(list, WM_CHAR, usize::from(b' '), 0);
            assert_eq!(form.list_checked(LIST)[0], checked);
            assert_eq!(
                native_name(list, 0),
                format!(
                    "Fixture 0 (USB), checked, {}",
                    if checked { "checked" } else { "unchecked" }
                )
            );
            assert_eq!(message(list, LB_GETCURSEL, 0, 0), 0);
        }
        // A native mouse click exercises selection notification and CheckOnClick.
        message(list, WM_LBUTTONDOWN, 1, 5 | (5 << 16));
        message(list, WM_LBUTTONUP, 0, 5 | (5 << 16));
        assert!(form.list_checked(LIST)[0]);
        assert_eq!(native_name(list, 0), "Fixture 0 (USB), checked, checked");
        message(list, LB_SETCURSEL, 40, 0);
        message(list, LB_SETTOPINDEX, 35, 0);
        let top = message(list, LB_GETTOPINDEX, 0, 0);
        form.list_set_checked(LIST, 0, false);
        assert_eq!(native_name(list, 0), "Fixture 0 (USB), checked, unchecked");
        assert_eq!(message(list, LB_GETCURSEL, 0, 0), 40);
        assert_eq!(message(list, LB_GETTOPINDEX, 0, 0), top);
        // The same setter is used by Reset; idempotent updates retain name and state.
        for i in 0..items.len() {
            form.list_set_checked(LIST, i, false);
        }
        assert!(form.list_checked(LIST).iter().all(|checked| !checked));
        assert_eq!(native_name(list, 1), "Fixture 1 (USB), checked, unchecked");
        controls::list_set_items(list, &items[..2]);
        let directory = std::path::Path::new("D:/GIT/HWID-Privacy/app/rust/golden/minors-2-ui");
        std::fs::create_dir_all(directory).expect("evidence directory");
        msgbox::testing::capture(form.hwnd(), &directory.join("whitelist.png"))
            .expect("whitelist capture");
        form.destroy();
        println!(
            "PASS LB_GETTEXT: initial, Space twice, mouse, setter/reset; selection/top preserved"
        );
    }
}
