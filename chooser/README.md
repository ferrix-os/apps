# chooser

Ferrix's file chooser: tole's `FileDialog` (`Fx.Controls`, after the
customer's Key File Dialog design) in a window of its own. `portald`, the
desktop portal, opens it for `org.freedesktop.portal.FileChooser`, so on a
Yoru desktop GTK 3 (`GTK_USE_PORTAL=1`), GTK 4 (`GDK_DEBUG=portals`) and
Chrome open and save through it instead of GTK's own dialog.

    chooser [--save] [--title T] [--name N] [--folder DIR]
            [--accept PATTERN] [--accept-label L] [--action L]

The path chosen goes to standard output with status 0; a cancel is status 1
and no output. `--accept` is the dialog's own pattern (`*.png | [image]`),
which portald makes from the portal's globs and MIME types. `--render FILE`
draws the dialog into a PNG instead, as `screenshot.png` was.

Not yet: one file is chosen where `multiple` asks for several, and a folder
is not chosen as such.
