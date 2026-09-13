%global forgeurl https://gitlab.postmarketos.org/postmarketOS/msm-firmware-loader
# %changelog is intentionally empty; don't derive SOURCE_DATE_EPOCH from it.
%global source_date_epoch_from_changelog 0

Name:           msm-firmware-loader
# overwritten from BASE.env by build.sh
Version:        0
Release:        1%{?dist}.armada
Summary:        Loads DSP/modem/WiFi firmware from Qualcomm firmware partitions

License:        MIT
URL:            %{forgeurl}
Source0:        %{forgeurl}/-/archive/%{commit}/%{name}-%{commit}.tar.gz

Patch1:         0001-load-hexagonrpcd-firmware.patch

BuildArch:      noarch
BuildRequires:  systemd-rpm-macros
Requires:       qbootctl
Requires:       make-dynpart-mappings

%{?systemd_requires}

%description
msm-firmware-loader mounts a Qualcomm device's dedicated firmware partitions
(modem, dsp, bluetooth, persist, vendor, ...) at early boot and symlinks the
blobs it finds into a single tree, which it then points the kernel's
firmware_class loader at. This lets one rootfs boot on multiple devices
without baking in per-device firmware.

%prep
%autosetup -n %{name}-%{commit} -p1

%build

%install
install -Dpm 0755 msm-firmware-loader.sh %{buildroot}%{_sbindir}/msm-firmware-loader.sh
install -Dpm 0755 msm-firmware-loader-unpack.sh %{buildroot}%{_sbindir}/msm-firmware-loader-unpack.sh
install -Dpm 0644 msm-firmware-loader.service %{buildroot}%{_unitdir}/msm-firmware-loader.service
install -Dpm 0644 msm-firmware-loader-unpack.service %{buildroot}%{_unitdir}/msm-firmware-loader-unpack.service

%post
%systemd_post msm-firmware-loader.service msm-firmware-loader-unpack.service

%preun
%systemd_preun msm-firmware-loader.service msm-firmware-loader-unpack.service

%postun
%systemd_postun_with_restart msm-firmware-loader.service msm-firmware-loader-unpack.service

%files
%license LICENSE
%doc README.md
%{_sbindir}/msm-firmware-loader.sh
%{_sbindir}/msm-firmware-loader-unpack.sh
%{_unitdir}/msm-firmware-loader.service
%{_unitdir}/msm-firmware-loader-unpack.service

%changelog
