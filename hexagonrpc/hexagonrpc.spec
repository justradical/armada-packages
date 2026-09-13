%global forgeurl https://github.com/linux-msm/hexagonrpc
# %changelog is intentionally empty; don't derive SOURCE_DATE_EPOCH from it.
%global source_date_epoch_from_changelog 0

Name:           hexagonrpc
# overwritten from BASE.env by build.sh
Version:        0
Release:        1%{?dist}.armada
Summary:        FastRPC library and reverse-tunnel daemon for Qualcomm DSPs

License:        GPL-3.0-or-later
URL:            %{forgeurl}
Source0:        %{forgeurl}/archive/%{commit}/%{name}-%{commit}.tar.gz
Source1:        60-hexagonrpc.rules

Patch1:         0001-data-install-units-to-the-canonical-systemd-unit-dir.patch
Patch2:         0002-use-msm-firmware-loader-dir.patch

BuildRequires:  gcc
BuildRequires:  meson >= 1.1
BuildRequires:  ninja-build
BuildRequires:  pkgconfig(systemd)
BuildRequires:  systemd-rpm-macros

Requires(pre):  shadow-utils
Recommends:     msm-firmware-loader
%{?systemd_requires}

%description
HexagonRPC talks FastRPC to the Context Hub Runtime Environment running on a
Qualcomm DSP, serving files to it and relaying its remote procedure calls
back to a listener on the application processor.

%prep
%autosetup -n %{name}-%{commit} -p1

%build
%meson
%meson_build

%install
%meson_install
install -Dpm 0644 %{SOURCE1} %{buildroot}%{_prefix}/lib/udev/rules.d/60-hexagonrpc.rules

%pre
getent group fastrpc >/dev/null || groupadd -r fastrpc
getent passwd fastrpc >/dev/null || \
    useradd -r -g fastrpc -d / -s /sbin/nologin \
    -c "FastRPC DSP RPC daemon" fastrpc
exit 0

%post
%systemd_post hexagonrpcd-adsp-rootpd.service hexagonrpcd-adsp-sensorspd.service hexagonrpcd-sdsp.service

%preun
%systemd_preun hexagonrpcd-adsp-rootpd.service hexagonrpcd-adsp-sensorspd.service hexagonrpcd-sdsp.service

%postun
%systemd_postun_with_restart hexagonrpcd-adsp-rootpd.service hexagonrpcd-adsp-sensorspd.service hexagonrpcd-sdsp.service

%files
%license COPYING
%doc README.md
%{_bindir}/hexagonrpcd
%dir %{_libexecdir}/hexagonrpc
%{_libexecdir}/hexagonrpc/chrecd
%{_libdir}/libhexagonrpc.so*
%{_unitdir}/hexagonrpcd-adsp-rootpd.service
%{_unitdir}/hexagonrpcd-adsp-sensorspd.service
%{_unitdir}/hexagonrpcd-sdsp.service
%{_mandir}/man1/hexagonrpcd.1*
%{_prefix}/lib/udev/rules.d/60-hexagonrpc.rules

%changelog
