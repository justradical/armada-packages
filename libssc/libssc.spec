%global forgeurl https://codeberg.org/DylanVanAssche/libssc
# %changelog is intentionally empty; don't derive SOURCE_DATE_EPOCH from it.
%global source_date_epoch_from_changelog 0

Name:           libssc
# overwritten from BASE.env by build.sh
Version:        0
Release:        1%{?dist}.armada
Summary:        Library to access sensors managed by the Qualcomm Sensor Core

License:        GPL-3.0-or-later
URL:            %{forgeurl}
Source0:        %{forgeurl}/archive/%{commit}.tar.gz#/%{name}-%{commit}.tar.gz

Patch1:         0001-meson-drop-the-mocking-and-tests-subdirs.patch

BuildRequires:  gcc
BuildRequires:  meson >= 1.4.0
BuildRequires:  ninja-build
BuildRequires:  pkgconfig(glib-2.0) >= 2.56
BuildRequires:  pkgconfig(gio-2.0)
BuildRequires:  pkgconfig(gio-unix-2.0)
BuildRequires:  pkgconfig(gobject-2.0)
BuildRequires:  pkgconfig(qmi-glib) >= 1.33.4
BuildRequires:  pkgconfig(libprotobuf-c)
BuildRequires:  protobuf-compiler
BuildRequires:  /usr/bin/protoc-gen-c

%description
Qualcomm SoCs offload sensors to a dedicated Sensor Low Power Island (SLPI)
DSP; direct access is blocked by the hypervisor, so the only way to reach
these sensors is by talking QMI to the DSP over QRTR. libssc does that and
exposes proximity, light, accelerometer, magnetometer, gyroscope, and
rotation-vector sensors as a GLib-based library.

%package        devel
Summary:        Development files for %{name}
Requires:       %{name}%{?_isa} = %{version}-%{release}

%description    devel
Headers and pkgconfig file for developing applications against %{name}.

%prep
%autosetup -n %{name} -p1

%build
%meson
%meson_build

%install
%meson_install

%files
%license LICENSE
%doc README.md CHANGELOG.md
%{_bindir}/ssccli
%{_libdir}/libssc.so.*

%files devel
%{_includedir}/%{name}/
%{_libdir}/libssc.so
%{_libdir}/pkgconfig/libssc.pc

%changelog
