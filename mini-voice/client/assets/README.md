# Client icon

Original AI-generated illustration of an adult feminine anime character with a
voice headset, created for Mini Voice. Distributed under the project's MIT
license. `client.png` is the source artwork; `client.ico` contains Windows icon
sizes 16, 24, 32, 48, 64, 128 and 256 pixels.

The build script embeds resource ID 1 in the executable. The native window uses
that resource for its title bar and taskbar icon. No external image is needed
when running the client.

To regenerate the ICO with ImageMagick:

```sh
magick client.png -define icon:auto-resize=256,128,64,48,32,24,16 client.ico
```

Windows builds require Windows SDK `rc.exe` (MSVC) or MinGW `windres` (GNU;
optionally set `WINDRES` to its path).
