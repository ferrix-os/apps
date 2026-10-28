//! The window's tree, in tole: the markup under `ui/` is drawn from what
//! [`View::show`] hands it of the [`App`], and what is pressed in it comes
//! back from [`View::actions`] as [`Action`]s.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use tole::{Color, Element as _, FileKind, ItemId, Ui};

use crate::app::{Action, App, DATE_CHIPS, Dialog, Item, KIND_CHIPS, Loc, SIZE_CHIPS, count_of};
use crate::archive::Format;
use crate::store::tag_color;

tole::include_ui!();

/// The fonts shelf draws with: `SHELF_FONTS` when set, the Ferrix tree's
/// Inter beside the sources (for its screenshots), then the system's.
#[must_use]
pub fn fonts() -> tole::Fonts {
    if let Some(dir) = std::env::var_os("SHELF_FONTS") {
        let mut fonts = tole::Fonts::new();
        let _ = fonts.add_dir(Path::new(&dir));
        return fonts;
    }
    let tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../assets/fonts/inter");
    if tree.is_dir() && std::env::var_os("SHELF_SYSTEM_FONTS").is_none() {
        let mut fonts = tole::Fonts::new();
        let _ = fonts.add_dir(&tree);
        return fonts;
    }
    tole::Fonts::system()
}

fn rc(text: impl AsRef<str>) -> Rc<str> {
    Rc::from(text.as_ref())
}

fn color(rgba: u32) -> Color {
    Color::rgba(
        ((rgba >> 24) & 0xff) as u8,
        ((rgba >> 16) & 0xff) as u8,
        ((rgba >> 8) & 0xff) as u8,
        (rgba & 0xff) as u8,
    )
}

/// The text fields the program fills itself, by their placeholder.
#[derive(Clone, Copy, Debug, Default)]
struct Inputs {
    search: Option<ItemId>,
    name: Option<ItemId>,
    base: Option<ItemId>,
    start: Option<ItemId>,
    find: Option<ItemId>,
    with: Option<ItemId>,
}

/// The tree and what the program keeps of it.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// Its root.
    pub shelf: Shelf,
    /// The dialog's popup.
    dialog: Option<ItemId>,
    inputs: Inputs,
    /// Whether the dialog was open at the last show.
    dialog_shown: bool,
    /// Where each pane's breadcrumbs go.
    crumbs: [Vec<Loc>; 2],
    /// The places down the side, the quick ones, the tags.
    places: Vec<PathBuf>,
    quick: Vec<PathBuf>,
    tags: Vec<String>,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

/// The first `TextInput` under `item` beside a `Text` saying `placeholder`.
fn input_by_placeholder(ui: &mut Ui, root: ItemId, placeholder: &str) -> Option<ItemId> {
    let mut stack = vec![root];
    while let Some(item) = stack.pop() {
        let children = ui.children(item).to_vec();
        let has_text = children
            .iter()
            .any(|&c| ui.text(c).is_some_and(|t| &*ui.get(t.text) == placeholder));
        if has_text && let Some(&input) = children.iter().find(|&&c| ui.text_input(c).is_some()) {
            return Some(input);
        }
        stack.extend(children.into_iter().rev());
    }
    None
}

impl View {
    /// The window's tree, drawn with `fonts`.
    #[must_use]
    pub fn new(fonts: tole::Fonts) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let shelf = Shelf::new(&mut ui, None)?;
        let root = shelf.id();
        let mut dialog = None;
        let mut stack = vec![root];
        while let Some(item) = stack.pop() {
            if let Some(popup) = ui.popup(item)
                && ui.get(popup.modal)
            {
                dialog = Some(item);
            }
            stack.extend(ui.children(item).iter().copied());
        }
        let inputs = Inputs {
            search: input_by_placeholder(&mut ui, root, "Filter by name"),
            name: input_by_placeholder(&mut ui, root, "New name"),
            base: input_by_placeholder(&mut ui, root, "Name"),
            start: input_by_placeholder(&mut ui, root, "Start at"),
            find: input_by_placeholder(&mut ui, root, "Find"),
            with: input_by_placeholder(&mut ui, root, "Replace with"),
        };
        Some(View {
            ui,
            shelf,
            dialog,
            inputs,
            dialog_shown: false,
            crumbs: [Vec::new(), Vec::new()],
            places: Vec::new(),
            quick: Vec::new(),
            tags: Vec::new(),
        })
    }

    fn set_input(&mut self, input: Option<ItemId>, text: &str) {
        if let Some(props) = input.and_then(|id| self.ui.text_input(id))
            && &*self.ui.get(props.text) != text
        {
            props.text.set(&mut self.ui, rc(text));
            props
                .cursor_position
                .set(&mut self.ui, i32::try_from(text.len()).unwrap_or(0));
        }
    }

    /// Whether a text field has the keys.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.ui
            .active_focus_item()
            .is_some_and(|item| self.ui.text_input(item).is_some())
    }

    /// Whether the dialog is open.
    #[must_use]
    pub fn dialog_open(&self) -> bool {
        self.dialog_shown
    }

    /// Hand the tree what `app` holds now.
    #[allow(
        clippy::too_many_lines,
        reason = "one property after another, in the window's order"
    )]
    pub fn show(&mut self, app: &mut App) {
        let ui = &mut self.ui;
        let s = self.shelf;
        let theme = fx_controls::Theme::instance(ui);
        theme.dark.set(ui, app.settings.dark);

        // Tabs.
        let tabs: Vec<TabRow> = app
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let first = loc_name(&tab.panes[0].loc);
                let (label, badge) = if tab.dual {
                    (
                        format!("{first} | {}", loc_name(&tab.panes[1].loc)),
                        "2\u{d7}",
                    )
                } else {
                    (first, loc_badge(&tab.panes[0].loc))
                };
                TabRow {
                    label: rc(label),
                    badge: rc(badge),
                    active: i == app.active,
                }
            })
            .collect();
        s.tabs.set(ui, tabs);

        let focus = app.focus();
        let pane = app.pane(focus).clone();
        s.grid.set(ui, app.settings.grid);
        s.dual.set(ui, app.tab().dual);
        s.focus.set(ui, i32::try_from(focus).unwrap_or(0));
        s.can_back.set(ui, pane.can_back());
        s.can_forward.set(ui, pane.can_forward());
        s.search_mode.set(ui, app.settings.search_mode);
        let error = tole::Search::new(&pane.query, app.settings.search_mode)
            .err()
            .unwrap_or_default();
        s.search_error.set(ui, rc(&error));
        let tag_names: Vec<String> = app.tags.all().into_iter().map(|(t, _)| t).collect();
        s.chip_kind.set(
            ui,
            rc(pane
                .chips
                .kind
                .and_then(|i| KIND_CHIPS.get(i))
                .copied()
                .unwrap_or("")),
        );
        s.chip_date.set(
            ui,
            rc(pane
                .chips
                .date
                .and_then(|i| DATE_CHIPS.get(i))
                .copied()
                .unwrap_or("")),
        );
        s.chip_size.set(
            ui,
            rc(pane
                .chips
                .size
                .and_then(|i| SIZE_CHIPS.get(i))
                .copied()
                .unwrap_or("")),
        );
        s.chip_tag
            .set(ui, rc(pane.chips.tag.clone().unwrap_or_default()));
        s.tag_names.set(ui, tag_names.iter().map(rc).collect());
        s.show_hidden.set(ui, app.settings.hidden);
        s.folders_first.set(ui, app.settings.folders_first);
        s.preview_on.set(ui, app.settings.preview);
        s.extensions.set(ui, app.settings.extensions);
        s.folder_sizes.set(ui, app.settings.folder_sizes);
        s.compact.set(ui, app.settings.compact);
        s.sort_column.set(ui, rc(&app.settings.sort));
        s.ascending.set(ui, app.settings.ascending);
        self.tags = tag_names;

        // The sidebar.
        let here = pane.loc.clone();
        self.quick = app.quick.paths.clone();
        let quick: Vec<Side> = self
            .quick
            .iter()
            .map(|p| {
                let is_dir = p.is_dir();
                let name = p.file_name().map_or_else(
                    || p.to_string_lossy().into_owned(),
                    |n| n.to_string_lossy().into_owned(),
                );
                let kind = if is_dir {
                    FileKind::Dir
                } else {
                    tole::files::classify(p, &name, false, 0)
                };
                Side {
                    name: rc(name),
                    badge: rc(kind.badge()),
                    kind: rc(kind.name()),
                    detail: rc(""),
                    active: here == Loc::Dir(p.clone()),
                    dot: Color::TRANSPARENT,
                }
            })
            .collect();
        s.quick.set(ui, quick);
        let home = app.home.clone();
        let mut places = vec![("Home".to_owned(), home.clone())];
        for (name, rel) in [
            ("Machine keys", "machines/keys"),
            (".ssh", ".ssh"),
            ("Pictures", "Pictures"),
            ("Documents", "Documents"),
            ("Downloads", "Downloads"),
            ("Music", "Music"),
            ("Videos", "Videos"),
        ] {
            let path = home.join(rel);
            if path.is_dir() {
                places.push((name.to_owned(), path));
            }
        }
        let sizes_on = app.settings.folder_sizes;
        let place_rows: Vec<Side> = places
            .iter()
            .map(|(name, path)| {
                let detail = if sizes_on && *path != home {
                    app.sizes
                        .get(path)
                        .map(|t| tole::files::size_text(t.bytes))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                Side {
                    name: rc(name),
                    badge: rc("DIR"),
                    kind: rc("folder"),
                    detail: rc(detail),
                    active: here == Loc::Dir(path.clone()),
                    dot: Color::TRANSPARENT,
                }
            })
            .collect();
        self.places = places.into_iter().map(|(_, p)| p).collect();
        s.places.set(ui, place_rows);
        let tag_rows: Vec<Side> = app
            .tags
            .all()
            .into_iter()
            .map(|(tag, n)| Side {
                active: here == Loc::Tag(tag.clone()),
                name: rc(&tag),
                badge: rc(""),
                kind: rc(""),
                detail: rc(n.to_string()),
                dot: color(tag_color(&tag)),
            })
            .collect();
        s.tag_rows.set(ui, tag_rows);
        s.trash_detail.set(
            ui,
            rc(if app.trash.is_empty() {
                String::new()
            } else {
                app.trash.len().to_string()
            }),
        );
        s.trash_active.set(ui, here == Loc::Trash);
        self.show_disk(app);

        // The panes.
        for n in 0..2 {
            self.show_pane(app, n);
        }

        // The preview and the foot.
        self.show_preview(app);
        let ui = &mut self.ui;
        let items = app.items(focus).unwrap_or_default();
        let total: u64 = items.iter().filter(|i| !i.is_dir).map(|i| i.bytes).sum();
        s.status_text.set(
            ui,
            rc(format!(
                "{} \u{b7} {}",
                count_of(items.len()),
                tole::files::size_text(total)
            )),
        );
        let chosen = app.chosen();
        let chosen_bytes: u64 = chosen.iter().map(|i| i.bytes).sum();
        s.selection_text.set(
            ui,
            rc(if chosen.is_empty() {
                String::new()
            } else {
                format!(
                    "{} selected \u{b7} {}",
                    chosen.len(),
                    tole::files::size_text(chosen_bytes)
                )
            }),
        );
        s.toast.set(ui, rc(app.toast.clone().unwrap_or_default()));
        let query = pane.query.clone();
        self.set_input(self.inputs.search, &query);
        self.show_dialog(app);
    }

    fn show_disk(&mut self, app: &mut App) {
        let ui = &mut self.ui;
        let s = self.shelf;
        let disk = app.disk;
        if disk.total == 0 {
            s.disk_text.set(ui, rc(""));
            s.disk.set(ui, Vec::new());
            return;
        }
        s.disk_text.set(
            ui,
            rc(format!(
                "{} of {}",
                tole::files::size_text(disk.used),
                tole::files::size_text(disk.total)
            )),
        );
        let home = app.home.clone();
        let mut parts: Vec<(&str, u64, u32)> = Vec::new();
        let mut in_home = 0;
        if app.settings.folder_sizes {
            for (label, rel, rgba) in [
                ("Videos", "Videos", 0xf0a0_b4ff),
                ("Pictures", "Pictures", 0xc3a6_f5ff),
                ("Documents", "Documents", 0x7ea6_f6ff),
            ] {
                let path = home.join(rel);
                if path.is_dir()
                    && let Some(total) = app.sizes.get(&path)
                {
                    parts.push((label, total.bytes, rgba));
                    in_home += total.bytes;
                }
            }
            if let Some(all) = app.sizes.get(&home) {
                parts.push(("Other", all.bytes.saturating_sub(in_home), 0x7fd1_a8ff));
                in_home = all.bytes;
            }
        }
        parts.insert(
            0,
            (
                "System & apps",
                disk.used.saturating_sub(in_home),
                0x5d61_6aff,
            ),
        );
        let segs: Vec<Seg> = parts
            .into_iter()
            .map(|(label, bytes, rgba)| Seg {
                frac: (bytes as f64 / disk.total as f64) as f32,
                color: color(rgba),
                label: rc(label),
                size: rc(tole::files::size_text(bytes)),
            })
            .collect();
        s.disk.set(ui, segs);
    }

    fn show_pane(&mut self, app: &mut App, n: usize) {
        let s = self.shelf;
        let pane = app.pane(n).clone();
        let (rows, crumbs, place, count, empty, error) = match (&s.rows0, n) {
            (_, 0) => (s.rows0, s.crumbs0, s.place0, s.count0, s.empty0, s.error0),
            _ => (s.rows1, s.crumbs1, s.place1, s.count1, s.empty1, s.error1),
        };
        let listed = app.items(n);
        let (items, why) = match listed {
            Ok(items) => (items, String::new()),
            Err(why) => (Vec::new(), why),
        };
        let biggest = items
            .iter()
            .filter(|i| i.is_dir && i.size_known)
            .map(|i| i.bytes)
            .max()
            .unwrap_or(0);
        let grid = app.settings.grid;
        let row_list: Vec<FileRow> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let shown_name = if app.settings.extensions || item.is_dir {
                    item.name.clone()
                } else {
                    let ext = crate::fsops::extension(&item.name);
                    let stem = item
                        .name
                        .get(..item.name.len() - ext.len())
                        .unwrap_or(&item.name);
                    if stem.is_empty() {
                        item.name.clone()
                    } else {
                        stem.to_owned()
                    }
                };
                let tags: Vec<Color> = item
                    .tags
                    .iter()
                    .take(3)
                    .map(|t| color(tag_color(t)))
                    .collect();
                FileRow {
                    name: rc(shown_name),
                    badge: rc(item.badge()),
                    kind: rc(item.kind_name()),
                    kind_label: rc(item.kind_label()),
                    size_text: rc(size_of(item, app.settings.folder_sizes)),
                    bar: if item.is_dir
                        && item.size_known
                        && biggest > 0
                        && item.inner.is_none()
                        && app.settings.folder_sizes
                    {
                        (item.bytes as f64 / biggest as f64) as f32
                    } else {
                        -1.0
                    },
                    modified: rc(if item.mtime > 0 {
                        tole::files::date_text(item.mtime)
                    } else {
                        "\u{2014}".to_owned()
                    }),
                    selected: pane.selected.contains(&item.path),
                    primary: pane.primary.as_ref() == Some(&item.path),
                    dim: item.name.starts_with('.'),
                    image: rc(if item.is_image() && (!grid || i < 400) {
                        item.path.to_string_lossy().into_owned()
                    } else {
                        String::new()
                    }),
                    is_dir: item.is_dir,
                    tag_count: i32::try_from(tags.len()).unwrap_or(0),
                    tag1: tags.first().copied().unwrap_or(Color::TRANSPARENT),
                    tag2: tags.get(1).copied().unwrap_or(Color::TRANSPARENT),
                    tag3: tags.get(2).copied().unwrap_or(Color::TRANSPARENT),
                }
            })
            .collect();
        let total: u64 = items.iter().filter(|i| !i.is_dir).map(|i| i.bytes).sum();
        let (labels, targets) = crumbs_of(&pane.loc, &app.home);
        let ui = &mut self.ui;
        rows.set(ui, row_list);
        // Each crumb's path is the labels up to it, which tells the one
        // it is in (the last) from the rest; a click is mapped back by
        // its index.
        let mut at = String::new();
        let entries: Vec<tole::FileEntry> = labels
            .iter()
            .map(|label| {
                if !at.is_empty() {
                    at.push('/');
                }
                at.push_str(label);
                tole::FileEntry::named(label, &at)
            })
            .collect();
        place.set(ui, rc(&at));
        crumbs.set(ui, tole::ModelRc::new(tole::VecModel::new(entries)));
        count.set(
            ui,
            rc(if items.is_empty() {
                String::new()
            } else {
                count_of(items.len())
            }),
        );
        let _ = total;
        let empty_text = if !pane.query.is_empty() {
            format!("No matches for \u{201c}{}\u{201d}", pane.query)
        } else if pane.chips.any() {
            "Nothing here matches the chips. Clear them to see everything.".to_owned()
        } else if pane.loc == Loc::Trash {
            "The trash is empty.".to_owned()
        } else {
            "This folder is empty.".to_owned()
        };
        empty.set(ui, rc(empty_text));
        let search_error = tole::Search::new(&pane.query, app.settings.search_mode).err();
        error.set(ui, rc(search_error.map_or(why, |e| format!("Search: {e}"))));
        if let Some(kept) = self.crumbs.get_mut(n) {
            *kept = targets;
        }
    }

    fn show_preview(&mut self, app: &mut App) {
        let s = self.shelf;
        let chosen = app.chosen();
        let shown = app.settings.preview && !chosen.is_empty();
        let ui = &mut self.ui;
        s.preview_shown.set(ui, shown);
        if !shown {
            return;
        }
        let all_tags: Vec<String> = app.tags.all().into_iter().map(|(t, _)| t).collect();
        let mut tag_names = all_tags.clone();
        for item in &chosen {
            for t in &item.tags {
                if !tag_names.contains(t) {
                    tag_names.push(t.clone());
                }
            }
        }
        let chips: Vec<TagChip> = tag_names
            .iter()
            .map(|t| TagChip {
                name: rc(t),
                dot: color(tag_color(t)),
                on: chosen.iter().all(|i| i.tags.contains(t)),
            })
            .collect();
        let in_trash = chosen.iter().all(|i| i.trashed.is_some());
        let changeable = chosen
            .iter()
            .all(|i| i.inner.is_none() && i.trashed.is_none());
        let other = app.other_dir();
        let multi = chosen.len() > 1;
        let primary = app.primary().or_else(|| chosen.last().cloned());
        let ui = &mut self.ui;
        s.pv_multi.set(ui, multi);
        s.pv_tags
            .set(ui, if changeable { chips } else { Vec::new() });
        s.pv_in_trash.set(ui, in_trash);
        s.pv_can_change.set(ui, changeable);
        s.pv_can_transfer.set(ui, changeable && other.is_some());
        if multi {
            let bytes: u64 = chosen.iter().map(|i| i.bytes).sum();
            s.pv_name.set(ui, rc(count_of(chosen.len())));
            s.pv_kind.set(
                ui,
                rc(format!("{} together", tole::files::size_text(bytes))),
            );
            s.pv_badge.set(ui, rc(format!("{}\u{d7}", chosen.len())));
            s.pv_kind_name.set(ui, rc("folder"));
            s.pv_image.set(ui, rc(""));
            let names: Vec<&str> = chosen.iter().take(30).map(|i| i.name.as_str()).collect();
            s.pv_text.set(ui, rc(names.join("\n")));
            s.pv_text_dim.set(ui, false);
            s.pv_meta.set(
                ui,
                vec![Meta {
                    k: rc("Size"),
                    v: rc(tole::files::size_text(bytes)),
                }],
            );
            s.pv_pinned.set(ui, false);
            s.pv_can_extract.set(ui, false);
            s.pv_is_dir.set(ui, false);
            return;
        }
        let Some(item) = primary else {
            return;
        };
        let (text, dim) = preview_of(app, &item);
        let mut meta = Vec::new();
        if item.is_dir {
            meta.push(Meta {
                k: rc("Items"),
                v: rc(item.entries.to_string()),
            });
            if item.size_known && item.inner.is_none() {
                meta.push(Meta {
                    k: rc("Size"),
                    v: rc(tole::files::size_text(item.bytes)),
                });
            }
        } else {
            meta.push(Meta {
                k: rc("Size"),
                v: rc(tole::files::size_text(item.bytes)),
            });
        }
        if item.mtime > 0 {
            meta.push(Meta {
                k: rc("Modified"),
                v: rc(tole::files::date_text(item.mtime)),
            });
        }
        if let Some(trashed) = &item.trashed {
            let from = trashed
                .original
                .parent()
                .map(|p| tole::files::tilde(&p.to_string_lossy()))
                .unwrap_or_default();
            meta.push(Meta {
                k: rc("From"),
                v: rc(from),
            });
            meta.push(Meta {
                k: rc("Deleted"),
                v: rc(trashed.deleted.replace('T', " ")),
            });
        } else {
            let location = item
                .path
                .parent()
                .map(|p| tole::files::tilde(&p.to_string_lossy()))
                .unwrap_or_default();
            meta.push(Meta {
                k: rc("Location"),
                v: rc(location),
            });
        }
        let readable = matches!(
            item.archive,
            Some(Format::Zip | Format::Tar | Format::TarGz)
        );
        if readable && let Ok(index) = app.archive(&item.path) {
            meta.push(Meta {
                k: rc("Entries"),
                v: rc(index.entries.len().to_string()),
            });
            meta.push(Meta {
                k: rc("Unpacked"),
                v: rc(tole::files::size_text(index.unpacked)),
            });
        }
        if item.kind == FileKind::Private {
            meta.push(Meta {
                k: rc("Contents"),
                v: rc("Hidden for security"),
            });
        }
        let ui = &mut self.ui;
        s.pv_name.set(ui, rc(&item.name));
        s.pv_kind.set(ui, rc(item.kind_label()));
        s.pv_badge.set(ui, rc(item.badge()));
        s.pv_kind_name.set(ui, rc(item.kind_name()));
        s.pv_image.set(
            ui,
            rc(if item.is_image() {
                item.path.to_string_lossy().into_owned()
            } else {
                String::new()
            }),
        );
        s.pv_text.set(ui, rc(text));
        s.pv_text_dim.set(ui, dim);
        s.pv_meta.set(ui, meta);
        s.pv_pinned.set(ui, app.quick.has(&item.path));
        s.pv_can_extract.set(
            ui,
            readable && item.inner.is_none() && item.trashed.is_none(),
        );
        s.pv_is_dir.set(ui, item.is_dir);
    }

    fn show_dialog(&mut self, app: &App) {
        let s = self.shelf;
        let kind = match &app.dialog {
            None => 0,
            Some(Dialog::Confirm { .. }) => 1,
            Some(Dialog::Rename { .. }) => 2,
            Some(Dialog::Bulk { .. }) => 3,
            Some(Dialog::NewFolder { .. }) => 4,
        };
        let opening = kind != 0 && !self.dialog_shown;
        let ui = &mut self.ui;
        s.dialog.set(ui, kind);
        s.dlg_problem
            .set(ui, rc(app.dialog_problem().unwrap_or_default()));
        match &app.dialog {
            Some(Dialog::Confirm {
                title,
                body,
                names,
                ok,
                danger,
                ..
            }) => {
                s.dlg_title.set(ui, rc(title));
                s.dlg_body.set(ui, rc(body));
                s.dlg_names.set(ui, names.iter().map(rc).collect());
                s.dlg_ok.set(ui, rc(ok));
                s.dlg_danger.set(ui, *danger);
            }
            Some(Dialog::Rename { text, .. }) => {
                s.dlg_title.set(ui, rc("Rename"));
                s.dlg_ok.set(ui, rc("Rename"));
                if opening {
                    let text = text.clone();
                    self.set_input(self.inputs.name, &text);
                }
            }
            Some(Dialog::NewFolder { text, .. }) => {
                s.dlg_title.set(ui, rc("New folder"));
                s.dlg_ok.set(ui, rc("Make it"));
                if opening {
                    let text = text.clone();
                    self.set_input(self.inputs.name, &text);
                }
            }
            Some(Dialog::Bulk {
                paths,
                format,
                find,
                with,
                base,
                start,
            }) => {
                s.dlg_title
                    .set(ui, rc(format!("Rename {}", count_of(paths.len()))));
                s.dlg_ok.set(ui, rc("Rename all"));
                s.bulk_format.set(ui, *format);
                let rows: Vec<BulkRow> = app
                    .bulk_plan()
                    .into_iter()
                    .map(|p| BulkRow {
                        old: rc(&p.old),
                        renamed: rc(&p.new),
                        bad: p.problem.is_some(),
                    })
                    .collect();
                s.bulk_rows.set(ui, rows);
                if opening {
                    let (find, with, base, start) =
                        (find.clone(), with.clone(), base.clone(), start.to_string());
                    self.set_input(self.inputs.find, &find);
                    self.set_input(self.inputs.with, &with);
                    self.set_input(self.inputs.base, &base);
                    self.set_input(self.inputs.start, &start);
                }
            }
            None => {}
        }
        if let Some(popup) = self.dialog {
            if kind != 0 && !self.dialog_shown {
                self.ui.open_popup(popup);
                let first = match kind {
                    2 | 4 => self.inputs.name,
                    3 => self.inputs.base,
                    _ => None,
                };
                if let Some(input) = first {
                    self.ui.force_active_focus(input);
                    self.ui.select_all(input);
                }
            } else if kind == 0 && self.dialog_shown {
                self.ui.close_popup(popup);
            }
        }
        self.dialog_shown = kind != 0;
    }

    /// Whether the window draws its own frame -- the compositor answered
    /// `client_side` or draws none -- and so its tab row shows minimize,
    /// maximize and close and moves the window
    /// (`tole::window::Window::draws_own_frame`).
    pub fn set_own_frame(&mut self, on: bool) {
        self.shelf.own_frame.set(&mut self.ui, on);
    }

    /// What was pressed since the last call, as actions.
    pub fn actions(&mut self) -> Vec<Action> {
        let emitted = self.ui.take_signals();
        let (shift, control) = self.ui.modifiers();
        let mut out = Vec::new();
        let at = |i: i32| usize::try_from(i).unwrap_or(0);
        for each in &emitted {
            let Some(signal) = self.shelf.signal(each) else {
                continue;
            };
            match signal {
                ShelfSignal::TabClicked(i) => out.push(Action::Tab(at(i))),
                ShelfSignal::TabClosed(i) => out.push(Action::CloseTab(at(i))),
                ShelfSignal::NewTab => out.push(Action::NewTab),
                ShelfSignal::GridPicked(on) => out.push(Action::Grid(on)),
                ShelfSignal::DualToggled => out.push(Action::ToggleDual),
                ShelfSignal::Back => out.push(Action::Back),
                ShelfSignal::Forward => out.push(Action::Forward),
                ShelfSignal::Up(n) => out.push(Action::Up(at(n))),
                ShelfSignal::QueryEdited(text) => out.push(Action::Query(text.to_string())),
                ShelfSignal::ModePicked(m) => out.push(Action::SearchMode(m)),
                ShelfSignal::ChipPicked(which, i) => {
                    out.push(Action::Chip(which.to_string(), usize::try_from(i).ok()))
                }
                ShelfSignal::ChipsCleared => out.push(Action::ClearChips),
                ShelfSignal::SettingToggled(name) => out.push(Action::Setting(name.to_string())),
                ShelfSignal::SortBy(column) => out.push(Action::Sort(column.to_string())),
                ShelfSignal::RowClicked(n, i) => {
                    out.push(Action::Click(at(n), at(i), shift, control))
                }
                ShelfSignal::RowActivated(n, i) => out.push(Action::Activate(at(n), at(i))),
                ShelfSignal::CrumbClicked(n, i) => {
                    if let Some(loc) = self.crumbs.get(at(n)).and_then(|c| c.get(at(i))) {
                        out.push(Action::Focus(at(n)));
                        out.push(Action::Go(loc.clone()));
                    }
                }
                ShelfSignal::PaneFocused(n) => out.push(Action::Focus(at(n))),
                ShelfSignal::PaneClosed(n) => out.push(Action::ClosePane(at(n))),
                ShelfSignal::PlaceClicked(i) => {
                    if let Some(p) = self.places.get(at(i)) {
                        out.push(Action::Go(Loc::Dir(p.clone())));
                    }
                }
                ShelfSignal::QuickClicked(i) => {
                    // A pinned file opens its folder.
                    let dir = self.quick.get(at(i)).and_then(|p| folder_of(p));
                    out.extend(dir.map(|d| Action::Go(Loc::Dir(d))));
                }
                ShelfSignal::TagClicked(i) => {
                    if let Some(t) = self.tags.get(at(i)) {
                        out.push(Action::Go(Loc::Tag(t.clone())));
                    }
                }
                ShelfSignal::TrashClicked => out.push(Action::Go(Loc::Trash)),
                ShelfSignal::Act(name) => out.push(match &*name {
                    "open" => Action::OpenPrimary,
                    "pin" => Action::Pin,
                    "rename" => Action::Rename,
                    "copy" => Action::CopyOver,
                    "move" => Action::MoveOver,
                    "trash" => Action::Trash,
                    "extract" => Action::Extract,
                    "restore" => Action::Restore,
                    "delete" => Action::DeleteForever,
                    "empty" => Action::EmptyTrash,
                    "new-folder" => Action::NewFolder,
                    _ => Action::ClearSelection,
                }),
                ShelfSignal::TagToggled(tag) => out.push(Action::TagToggle(tag.to_string())),
                ShelfSignal::TagAdded(tag) => out.push(Action::TagAdd(tag.to_string())),
                ShelfSignal::DialogText(which, text) => {
                    out.push(Action::DialogText(which.to_string(), text.to_string()))
                }
                ShelfSignal::BulkPicked(on) => out.push(Action::BulkFormat(on)),
                ShelfSignal::DialogOk => out.push(Action::DialogOk),
                ShelfSignal::DialogCancel => out.push(Action::DialogCancel),
                ShelfSignal::MoveWindow => out.push(Action::WindowMove),
                ShelfSignal::MaximizeWindow => out.push(Action::WindowMaximize),
                ShelfSignal::MinimizeWindow => out.push(Action::WindowMinimize),
                ShelfSignal::CloseWindow => out.push(Action::WindowClose),
            }
        }
        out
    }
}

/// `path` if it is a folder, else the folder it is in.
fn folder_of(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        Some(path.to_path_buf())
    } else {
        path.parent().map(Path::to_path_buf)
    }
}

/// What a row says for its size.
fn size_of(item: &Item, folder_sizes: bool) -> String {
    if !item.is_dir {
        return tole::files::size_text(item.bytes);
    }
    if item.size_known && (folder_sizes || item.inner.is_some()) {
        return tole::files::size_text(item.bytes);
    }
    count_of(item.entries)
}

/// What the preview shows of `item` in text, and whether dimmed.
fn preview_of(app: &mut App, item: &Item) -> (String, bool) {
    if let Some(inner) = &item.inner {
        return (
            format!("Inside an archive: {inner}\n\nExtract it to read it."),
            true,
        );
    }
    if let Some(format) = item.archive {
        if let Format::Other(kind) = format {
            return (
                format!("A {kind} archive.\nshelf does not read inside these."),
                true,
            );
        }
        return match app.archive(&item.path) {
            Ok(index) => {
                let names: Vec<String> = index
                    .children("")
                    .iter()
                    .take(30)
                    .map(|e| {
                        if e.is_dir {
                            format!("{}/", e.path)
                        } else {
                            e.path.clone()
                        }
                    })
                    .collect();
                (names.join("\n"), false)
            }
            Err(why) => (why, true),
        };
    }
    let text = tole::files::preview_text(&item.path, item.kind, app.settings.hidden);
    if text.is_empty() {
        return ("No preview for this file.".to_owned(), true);
    }
    (text, item.kind == FileKind::Private)
}

/// A place's name, for its tab.
fn loc_name(loc: &Loc) -> String {
    match loc {
        Loc::Dir(dir) => {
            if *dir == tole::files::home() {
                "Home".to_owned()
            } else {
                dir.file_name()
                    .map_or_else(|| "/".to_owned(), |n| n.to_string_lossy().into_owned())
            }
        }
        Loc::Archive { file, inner } => {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if inner.is_empty() {
                name
            } else {
                inner.rsplit('/').next().unwrap_or(inner).to_owned()
            }
        }
        Loc::Tag(tag) => format!("#{tag}"),
        Loc::Trash => "Trash".to_owned(),
    }
}

fn loc_badge(loc: &Loc) -> &'static str {
    match loc {
        Loc::Dir(_) => "DIR",
        Loc::Archive { .. } => "ZIP",
        Loc::Tag(_) => "TAG",
        Loc::Trash => "BIN",
    }
}

/// A place's breadcrumbs, and where each goes.
fn crumbs_of(loc: &Loc, home: &Path) -> (Vec<String>, Vec<Loc>) {
    let dir_crumbs = |dir: &Path| -> (Vec<String>, Vec<Loc>) {
        let (mut labels, mut targets) = (Vec::new(), Vec::new());
        let (start, rest) = match dir.strip_prefix(home) {
            Ok(rest) => (home.to_path_buf(), rest.to_path_buf()),
            Err(_) => (
                PathBuf::from("/"),
                dir.strip_prefix("/").unwrap_or(dir).to_path_buf(),
            ),
        };
        labels.push(if start == home {
            "~".to_owned()
        } else {
            "/".to_owned()
        });
        targets.push(Loc::Dir(start.clone()));
        let mut at = start;
        for part in rest.components() {
            at.push(part);
            labels.push(part.as_os_str().to_string_lossy().into_owned());
            targets.push(Loc::Dir(at.clone()));
        }
        (labels, targets)
    };
    match loc {
        Loc::Dir(dir) => dir_crumbs(dir),
        Loc::Archive { file, inner } => {
            let (mut labels, mut targets) = file.parent().map(&dir_crumbs).unwrap_or_default();
            labels.push(
                file.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            targets.push(Loc::Archive {
                file: file.clone(),
                inner: String::new(),
            });
            let mut at = String::new();
            for part in inner.split('/').filter(|p| !p.is_empty()) {
                if !at.is_empty() {
                    at.push('/');
                }
                at.push_str(part);
                labels.push(part.to_owned());
                targets.push(Loc::Archive {
                    file: file.clone(),
                    inner: at.clone(),
                });
            }
            (labels, targets)
        }
        Loc::Tag(tag) => (
            vec!["Tags".to_owned(), tag.clone()],
            vec![Loc::Tag(tag.clone()), Loc::Tag(tag.clone())],
        ),
        Loc::Trash => (vec!["Trash".to_owned()], vec![Loc::Trash]),
    }
}

#[cfg(test)]
mod tests {
    use super::View;
    use crate::app::{Action, App, Dialog, Loc};

    fn find_text(ui: &mut tole::Ui, label: &str) -> Option<tole::ItemId> {
        let mut stack: Vec<tole::ItemId> = ui.roots().to_vec();
        while let Some(item) = stack.pop() {
            let Some(common) = ui.common(item) else {
                continue;
            };
            if !ui.get(common.visible) {
                continue;
            }
            if let Some(text) = ui.text(item)
                && &*ui.get(text.text) == label
            {
                return Some(item);
            }
            stack.extend(ui.children(item).iter().rev());
        }
        None
    }

    /// Click the shown text `label`, then do what it asked.
    fn click(view: &mut View, app: &mut App, label: &str) {
        let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
        let item = find_text(&mut view.ui, label).unwrap_or_else(|| panic!("`{label}` is shown"));
        let rect = view.ui.rect_of(item).expect("placed");
        view.ui.pointer_move(rect.x + 4.0, rect.y + rect.h / 2.0);
        view.ui.pointer_press();
        view.ui.pointer_release();
        for action in view.actions() {
            app.act(action);
        }
        view.show(app);
    }

    #[test]
    fn the_screenshots_are_what_the_demo_draws() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("screenshots");
        for (dark, scene, name) in [
            (true, "main", "dark"),
            (false, "main", "light"),
            (true, "rename", "rename"),
            (false, "archive", "archive"),
            (false, "single", "single"),
            (true, "own-frame", "own-frame"),
        ] {
            let drawn = crate::cli::render_scene(dark, 1.0, scene).expect("drawn");
            let kept = std::fs::read(dir.join(format!("{name}.png"))).unwrap_or_default();
            assert!(
                drawn == kept,
                "screenshots/{name}.png is not what the demo draws: run `cargo run -- --render screenshots/{name}.png --scene {scene}{}`",
                if dark { "" } else { " --light" }
            );
        }
    }

    #[test]
    fn clicks_walk_choose_tag_and_rename_through_the_window() {
        let mut app = crate::demo::app(true);
        let home = crate::demo::home();
        let mut view = View::new(super::fonts()).expect("made");
        view.show(&mut app);
        // The other pane, then a folder in it.
        click(&mut view, &mut app, "invoice.pdf");
        assert_eq!(app.focus(), 1);
        assert_eq!(app.pane(1).selected, [home.join("Downloads/invoice.pdf")]);
        // A breadcrumb goes home.
        click(&mut view, &mut app, "~");
        let f = app.focus();
        assert_eq!(app.pane(f).loc, Loc::Dir(home.clone()));
        // A tag in the sidebar lists its files from everywhere.
        click(&mut view, &mut app, "staging");
        assert_eq!(app.pane(f).loc, Loc::Tag("staging".into()));
        let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
        assert!(find_text(&mut view.ui, "staging_rsa").is_some());
        // The archive in the documents opens as a folder.
        click(&mut view, &mut app, "Documents");
        let at = app
            .items(f)
            .expect("listed")
            .iter()
            .position(|i| i.name == "design-assets.tar.gz")
            .expect("there");
        app.act(Action::Activate(f, at));
        view.show(&mut app);
        let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
        assert!(find_text(&mut view.ui, "brand-guide.txt").is_some());
        assert!(find_text(&mut view.ui, "Fonts").is_some());
        // Two pictures chosen, then renamed together, numbered.
        click(&mut view, &mut app, "Pictures");
        click(&mut view, &mut app, "desk-plant.png");
        view.ui.set_modifiers(false, true);
        click(&mut view, &mut app, "harbour-dusk.png");
        view.ui.set_modifiers(false, false);
        assert_eq!(app.chosen().len(), 2, "a Control click adds");
        app.act(Action::Rename);
        view.show(&mut app);
        assert!(matches!(app.dialog, Some(Dialog::Bulk { .. })));
        assert!(view.dialog_open());
        let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
        assert!(
            find_text(&mut view.ui, "photo-01.png").is_some(),
            "the plan is shown"
        );
        click(&mut view, &mut app, "Cancel");
        assert!(app.dialog.is_none());
        // A search that cannot be read says so in the pane.
        app.act(Action::SearchMode(2));
        app.act(Action::Query("(".into()));
        view.show(&mut app);
        let error = view.ui.get(if app.focus() == 0 {
            view.shelf.error0
        } else {
            view.shelf.error1
        });
        assert!(error.starts_with("Search: Invalid regex"), "{error}");
    }

    /// Where every shown `Text` saying `label` is.
    fn texts_at(ui: &mut tole::Ui, label: &str) -> Vec<tole::Rect> {
        let mut found = Vec::new();
        let mut stack: Vec<tole::ItemId> = ui.roots().to_vec();
        while let Some(item) = stack.pop() {
            let Some(common) = ui.common(item) else {
                continue;
            };
            if !ui.get(common.visible) {
                continue;
            }
            if let Some(text) = ui.text(item)
                && &*ui.get(text.text) == label
                && let Some(rect) = ui.rect_of(item)
            {
                found.push(rect);
            }
            stack.extend(ui.children(item).iter().rev());
        }
        found
    }

    /// One pane shows where it is in the tool row, between the buttons and
    /// the search, and its list starts higher; two panes each show their
    /// own path bar over the pane, and the tool row has none. The tool
    /// row's crumbs still walk, and its up button still goes up.
    #[test]
    fn one_pane_has_its_path_bar_in_the_tool_row_and_two_have_their_own() {
        let mut app = crate::demo::app(false);
        let home = crate::demo::home();
        let mut view = View::new(super::fonts()).expect("made");
        view.show(&mut app);
        let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
        let split = texts_at(&mut view.ui, "~");
        assert_eq!(split.len(), 2, "a path bar in each pane: {split:?}");
        assert!(
            split.iter().all(|at| at.y > 136.0),
            "over the panes: {split:?}"
        );

        app.act(Action::ToggleDual);
        view.show(&mut app);
        let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
        let single = texts_at(&mut view.ui, "~");
        assert_eq!(single.len(), 1, "one path bar: {single:?}");
        let crumb = single[0];
        assert!(
            (54.0..90.0).contains(&crumb.y) && crumb.x > 290.0 && crumb.x < 1240.0 - 350.0,
            "in the tool row, between the buttons and the search: {crumb:?}"
        );
        // The list's head comes up where the pane's path bar was.
        let head = texts_at(&mut view.ui, "Name \u{2191}");
        assert!(
            head.iter().any(|at| at.y < 136.0 + 44.0),
            "the list starts at the pane's top: {head:?}"
        );
        // Its crumb walks: home.
        let f = app.focus();
        app.act(Action::Go(Loc::Dir(home.join("Pictures"))));
        view.show(&mut app);
        click(&mut view, &mut app, "~");
        assert_eq!(app.pane(f).loc, Loc::Dir(home.clone()));
        // Its up button goes up: from the pictures, home.
        app.act(Action::Go(Loc::Dir(home.join("Pictures"))));
        view.show(&mut app);
        let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
        view.ui.pointer_move(290.0 + 4.0 + 16.0, 54.0 + 18.0);
        view.ui.pointer_press();
        view.ui.pointer_release();
        let asked = view.actions();
        assert!(asked.contains(&Action::Up(0)), "the up button: {asked:?}");
    }

    /// The tab row is the window's title bar only where the window draws
    /// its own frame: then it has minimize, maximize and close at its
    /// right end, a press on its empty part moves the window and a double
    /// click maximizes it. Where the compositor draws the title bar (Yoru's)
    /// the row has none of that, and a press there asks nothing of the
    /// window.
    #[test]
    fn the_tab_row_is_the_title_bar_only_where_the_window_draws_its_frame() {
        let window_actions = |asked: &[Action]| -> Vec<Action> {
            asked
                .iter()
                .filter(|action| {
                    matches!(
                        action,
                        Action::WindowMove
                            | Action::WindowMaximize
                            | Action::WindowMinimize
                            | Action::WindowClose
                    )
                })
                .cloned()
                .collect()
        };
        let press = |view: &mut View, x: f32, y: f32, double: bool| -> Vec<Action> {
            let _ = view.ui.render((1240, 800), tole::Color::TRANSPARENT);
            view.ui.pointer_move(x, y);
            view.ui.pointer_press();
            view.ui.pointer_release();
            if double {
                view.ui.pointer_press();
                view.ui.pointer_release();
            }
            view.actions()
        };
        let spots = [1240.0 - 88.0, 1240.0 - 54.0, 1240.0 - 20.0];
        for own in [false, true] {
            let mut app = crate::demo::app(false);
            let mut view = View::new(super::fonts()).expect("made");
            view.set_own_frame(own);
            view.show(&mut app);
            let asked: Vec<Vec<Action>> = spots
                .iter()
                .map(|&x| window_actions(&press(&mut view, x, 22.0, false)))
                .collect();
            let empty = window_actions(&press(&mut view, 700.0, 22.0, false));
            let double = window_actions(&press(&mut view, 700.0, 22.0, true));
            if own {
                assert_eq!(
                    asked,
                    [
                        vec![Action::WindowMinimize],
                        vec![Action::WindowMaximize],
                        vec![Action::WindowClose]
                    ],
                    "its own buttons"
                );
                assert_eq!(empty, [Action::WindowMove], "the row moves the window");
                assert!(
                    double.contains(&Action::WindowMaximize),
                    "a double click maximizes: {double:?}"
                );
            } else {
                assert!(
                    asked.iter().all(Vec::is_empty) && empty.is_empty() && double.is_empty(),
                    "the compositor's bar has them: {asked:?} {empty:?} {double:?}"
                );
            }
            // A tab is still a tab.
            let tab = press(&mut view, 120.0, 26.0, false);
            assert!(tab.contains(&Action::Tab(0)), "{tab:?}");
            assert!(
                window_actions(&tab).is_empty(),
                "a tab is not the bar: {tab:?}"
            );
        }
    }
}
