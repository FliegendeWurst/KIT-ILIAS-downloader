with (import <nixpkgs> {});
mkShell {
  buildInputs = [
    dbus
  ];
}
