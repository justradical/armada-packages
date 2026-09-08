%global debug_package %{nil}
%global source_date_epoch_from_changelog 0

Name:           armada-bottom-touchpads
# Keep this in sync with Cargo.toml.
Version:        0.1.0
Release:        1%{?dist}.armada
Summary:        Bottom-screen touchpads bridge for Armada
License:        GPL-2.0-or-later
URL:            https://github.com/armada-os/armada-packages

Source0:        armada-bottom-touchpads.tar.gz

BuildRequires:  cargo
BuildRequires:  rust

%description
%{name} holds gamescope's DRM lease companion socket for a device's
bottom/secondary screen and forwards its touch input to InputPlumber as a
pair of Steam Deck style left/right touchpads, split down the center,
instead of running a full nested desktop session on that panel.

%prep
%autosetup -n work

%build
cargo build --release --locked

%install
install -Dpm 0755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
install -Dpm 0644 system/usr/lib/systemd/user/%{name}.service \
    %{buildroot}%{_prefix}/lib/systemd/user/%{name}.service

%files
%license LICENSE.md
%doc README.md
%{_bindir}/%{name}
%{_prefix}/lib/systemd/user/%{name}.service

%changelog
* Tue Sep 08 2026 Radical <radical@radical.fun> - 0.1.0-1
- Initial package
