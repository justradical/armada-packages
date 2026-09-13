%global forgeurl https://gitlab.com/flamingradian/make-dynpart-mappings
# %changelog is intentionally empty; don't derive SOURCE_DATE_EPOCH from it.
%global source_date_epoch_from_changelog 0

Name:           make-dynpart-mappings
# overwritten from BASE.env by build.sh
Version:        0
Release:        1%{?dist}.armada
Summary:        Sets up device-mapper targets for Android dynamic partitions

License:        GPL-3.0-only AND Apache-2.0
URL:            %{forgeurl}
Source0:        %{forgeurl}/-/archive/%{commit}/%{name}-%{commit}.tar.gz

BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  pkgconfig(devmapper)
BuildRequires:  pkgconfig(libmd)
BuildRequires:  pkgconfig(blkid)

%description
Android devices from Android 10 onward carve "dynamic partitions" (vendor,
system, product, odm, ...) out of a single super partition, described by
Google's liblp metadata format rather than a standard partition table.
make-dynpart-mappings reads that metadata and creates matching device-mapper
targets, so tools like msm-firmware-loader can mount those logical partitions
without needing Android's own fs_mgr.

%prep
%autosetup -n %{name}-%{commit}

%build
%set_build_flags
make %{?_smp_mflags} CFLAGS="%{build_cflags}" LDFLAGS="%{build_ldflags}"

%install
install -Dpm 0755 make-dynpart-mappings %{buildroot}%{_bindir}/make-dynpart-mappings

%files
%license LICENSE
%doc README.md
%{_bindir}/make-dynpart-mappings

%changelog
