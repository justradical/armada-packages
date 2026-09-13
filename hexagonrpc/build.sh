#!/usr/bin/bash

set -euxo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
PACKAGE_DIR="${PWD}"

source ./BASE.env
source ../toolchain.env

rm -rf out
mkdir -p out

podman run --rm \
  --volume "${PACKAGE_DIR}:/work:Z" \
  --workdir /work \
  --platform linux/aarch64 \
  --env COMMIT="${COMMIT}" \
  --env VERSION="${VERSION}" \
  "${BUILDER_IMAGE}" \
  bash -euxo pipefail -c '
    export HOME=/tmp
    dnf -y install rpm-build rpmdevtools spectool "dnf-command(builddep)" git-core
    rpmdev-setuptree
    cat >/etc/rpm/macros.armada <<EOF
%_buildhost armada-builder
%packager Armada
%vendor Armada
EOF
    cp /work/hexagonrpc.spec ~/rpmbuild/SPECS/
    sed -i "s/^Version:.*/Version:        ${VERSION}/" ~/rpmbuild/SPECS/hexagonrpc.spec
    cp /work/patches/*.patch ~/rpmbuild/SOURCES/
    cp /work/60-hexagonrpc.rules ~/rpmbuild/SOURCES/
    spectool -g -R --define "commit ${COMMIT}" ~/rpmbuild/SPECS/hexagonrpc.spec
    dnf -y builddep --define "commit ${COMMIT}" ~/rpmbuild/SPECS/hexagonrpc.spec
    rpmbuild -bb --define "commit ${COMMIT}" ~/rpmbuild/SPECS/hexagonrpc.spec
    cp ~/rpmbuild/RPMS/*/hexagonrpc-[0-9]*.armada.*.rpm /work/out/
  '

echo "built: ${PACKAGE_DIR}/out"
