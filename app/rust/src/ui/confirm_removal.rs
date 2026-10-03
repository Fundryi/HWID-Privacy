//! Owned by WP-14: device removal confirmation dialog (`DeviceRemovalConfirmationForm.cs`).

use super::controls::{Align, ButtonSpec, Ctl, LabelSpec};
use super::layout::{Anchor, FlowDir, Node, Size, Track};
use super::msgbox::{self, Buttons, Icon};
use super::theme;
use super::window::{self, Event, FormSpec, FormStyle, WindowSize};
use std::cell::Cell;
use std::rc::Rc;
use windows::Win32::Foundation::HWND;

const MESSAGE: u16 = 1;
const WARNING: u16 = 2;
const YES_AUTO_CLOSE: u16 = 3;
const YES: u16 = 4;
const NO: u16 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmResult {
    YesAutoClose,
    Yes,
    No,
}

/// Confirms the device count and optional automatic close behavior.
pub fn show(owner: HWND, count: usize) -> ConfirmResult {
    // C# parity: DeviceRemovalConfirmationForm.cs:26,31. Title-bar X and Alt+F4 leave No.
    let result = Rc::new(Cell::new(ConfirmResult::No));
    let r = Rc::clone(&result);
    let mut spec = FormSpec::new(
        "Confirm Device Removal",
        WindowSize::Client(theme::CONFIRM_CLIENT_SIZE),
    );
    spec.min = Some(theme::CONFIRM_MIN_SIZE);
    spec.style = FormStyle::FixedDialog;
    spec.back = theme::CONFIRM_BACKGROUND;
    spec.accept = Some(YES_AUTO_CLOSE);
    spec.cancel = Some(NO);
    let shown = window::run_modal(owner, spec, tree(count), move |form, event| {
        let handled = crate::win::catch_panic(|| match event {
            Event::Resize { client, .. } => {
                // C# parity: DeviceRemovalConfirmationForm.cs:186-191. Device pixels; the
                // 220 and 40 are not DPI scaled. Set again on every resize because a DPI
                // change rescales `max` from the logical values.
                let wrap = Size {
                    w: (client.w - theme::CONFIRM_LABEL_WRAP_INSET)
                        .max(theme::CONFIRM_LABEL_WRAP_MIN),
                    h: 0,
                };
                form.with_tree(|t| {
                    for id in [MESSAGE, WARNING] {
                        if let Some(n) = t.find_mut(id) {
                            n.max = wrap;
                        }
                    }
                });
            }
            Event::Click(id) => {
                r.set(match id {
                    YES_AUTO_CLOSE => ConfirmResult::YesAutoClose,
                    YES => ConfirmResult::Yes,
                    _ => ConfirmResult::No,
                });
                form.destroy();
            }
            _ => {}
        });
        if let Err(panic) = handled {
            // Nothing was confirmed: close as No and say why.
            r.set(ConfirmResult::No);
            msgbox::show(form.hwnd(), &panic, "Error", Buttons::Ok, Icon::Error);
            form.destroy();
        }
        true
    });
    if let Err(error) = shown {
        // The dialog never appeared, so nothing was confirmed.
        msgbox::show(owner, &error.to_string(), "Error", Buttons::Ok, Icon::Error);
        return ConfirmResult::No;
    }
    result.get()
}

fn tree(count: usize) -> Vec<Node> {
    let wrap = Size {
        w: (theme::CONFIRM_CLIENT_SIZE.w - theme::CONFIRM_LABEL_WRAP_INSET)
            .max(theme::CONFIRM_LABEL_WRAP_MIN),
        h: 0,
    };
    let label = |id: u16, text: &str, font, fore, margin| {
        Node::leaf(
            id,
            Ctl::Label(LabelSpec::new(text, font, fore).align(Align::MiddleCenter)),
        )
        .auto_size()
        .anchor(Anchor::NONE)
        .max(wrap)
        .margin(margin)
    };
    vec![
        Node::table(
            vec![Track::Percent(100.0)],
            vec![Track::AutoSize, Track::AutoSize, Track::AutoSize],
            vec![
                label(
                    MESSAGE,
                    &format!("Remove {count} ghost devices?"),
                    theme::CONFIRM_MESSAGE_FONT,
                    theme::CONFIRM_MESSAGE_TEXT,
                    theme::CONFIRM_MESSAGE_MARGIN,
                )
                .cell(0, 0),
                label(
                    WARNING,
                    "Warning: This action cannot be undone",
                    theme::CONFIRM_WARNING_FONT,
                    theme::CONFIRM_WARNING_TEXT,
                    theme::CONFIRM_WARNING_MARGIN,
                )
                .cell(0, 1),
                // Tab order 0, 1, 2 = add order; the first tab stop gets the initial focus.
                Node::flow(
                    FlowDir::LeftToRight,
                    true,
                    vec![
                        button(YES_AUTO_CLOSE, "Yes (Autoclose)", true),
                        button(YES, "Yes", false),
                        button(NO, "No", false),
                    ],
                )
                .auto_size()
                .anchor(Anchor::NONE)
                .margin(theme::NO_PAD)
                .cell(0, 2),
            ],
        )
        .fill()
        .padding(theme::CONFIRM_PADDING),
    ]
}

/// `CreateActionButton` (`DeviceRemovalConfirmationForm.cs:139-166`): the C# primary flag picks
/// the kit's primary kind (`Yes (Autoclose)` is the accept button), the others are outline.
fn button(id: u16, text: &str, primary: bool) -> Node {
    let (spec, min_w) = if primary {
        (ButtonSpec::primary(text), theme::CONFIRM_PRIMARY_MIN_WIDTH)
    } else {
        (
            ButtonSpec::outline(text),
            theme::CONFIRM_SECONDARY_MIN_WIDTH,
        )
    };
    Node::leaf(id, Ctl::Button(spec))
        .auto_size()
        .min(Size {
            w: min_w,
            h: theme::CONFIRM_BUTTON_HEIGHT,
        })
        .padding(theme::CONFIRM_BUTTON_PADDING)
        .margin(theme::CONFIRM_BUTTON_MARGIN)
}
