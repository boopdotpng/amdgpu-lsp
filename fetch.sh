#!/usr/bin/env bash
set -euo pipefail

out_dir="amd_gpu_xmls"
tmp_dir="$(mktemp -d)"
zip_path="${tmp_dir}/isa.zip"

mkdir -p "${out_dir}"

curl -L "https://gpuopen.com/download/machine-readable-isa/latest/" -o "${zip_path}"
unzip -o "${zip_path}" -d "${out_dir}"

echo "downloaded amdgpu ISA files to ${out_dir}"
