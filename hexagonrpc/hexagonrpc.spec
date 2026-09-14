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

Patch1:         0001-data-install-units-to-the-canonical-systemd-unit-dir.patch
Patch2:         0002-use-msm-firmware-loader-dir.patch
Patch3:         0003-run-hexagonrpcd-as-root.patch
Patch4:         0004-bring-hexagonrpcd-back-after-resume.patch

BuildRequires:  gcc
BuildRequires:  meson >= 1.1
BuildRequires:  ninja-build
BuildRequires:  pkgconfig(systemd)
BuildRequires:  systemd-rpm-macros

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

%post
%systemd_post hexagonrpcd-adsp-rootpd.service hexagonrpcd-adsp-sensorspd.service hexagonrpcd-sdsp.service hexagonrpcd-resume.service

%preun
%systemd_preun hexagonrpcd-adsp-rootpd.service hexagonrpcd-adsp-sensorspd.service hexagonrpcd-sdsp.service hexagonrpcd-resume.service

%postun
%systemd_postun_with_restart hexagonrpcd-adsp-rootpd.service hexagonrpcd-adsp-sensorspd.service hexagonrpcd-sdsp.service hexagonrpcd-resume.service

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
%{_unitdir}/hexagonrpcd-resume.service
%{_mandir}/man1/hexagonrpcd.1*

%changelog
