#!/bin/bash
set -euo pipefail

vmid=${AIRTEK_JENKINS_VMID:-107}
vm_name=${AIRTEK_JENKINS_VM_NAME:-airtek-jenkins}
storage=${AIRTEK_JENKINS_STORAGE:-ZTnvme}
bridge=${AIRTEK_JENKINS_BRIDGE:-vmbr0}
ssh_key=${1:-/root/deploy.pub}
image_name=debian-12-genericcloud-amd64.qcow2
image_root=https://cloud.debian.org/images/cloud/bookworm/latest
download_dir=/var/lib/vz/template/iso
image_path=$download_dir/$image_name
checksums_path=$download_dir/debian-12-SHA512SUMS

if [ "$(id -u)" -ne 0 ] || ! command -v qm >/dev/null 2>&1; then
  echo "Run this script as root on the Proxmox VE node." >&2
  exit 2
fi
if qm status "$vmid" >/dev/null 2>&1; then
  echo "VMID $vmid already exists; refusing to overwrite it." >&2
  exit 3
fi
if [ ! -r "$ssh_key" ]; then
  echo "SSH public key is not readable: $ssh_key" >&2
  exit 2
fi
pvesm status --storage "$storage" >/dev/null
ip link show "$bridge" >/dev/null
install -d -m 0755 "$download_dir"

curl -fL --retry 3 "$image_root/$image_name" -o "$image_path"
curl -fL --retry 3 "$image_root/SHA512SUMS" -o "$checksums_path"
expected=$(awk -v name="$image_name" '$2 == name { print $1 }' "$checksums_path")
if [ -z "$expected" ]; then
  echo "The official checksum list does not contain $image_name." >&2
  exit 4
fi
actual=$(sha512sum "$image_path" | awk '{ print $1 }')
if [ "$actual" != "$expected" ]; then
  echo "Debian cloud image checksum mismatch." >&2
  exit 4
fi
echo "Verified Debian SHA-512: $actual"

qm create "$vmid" \
  --name "$vm_name" \
  --machine q35 \
  --ostype l26 \
  --cpu host \
  --sockets 1 \
  --cores 12 \
  --memory 24576 \
  --balloon 0 \
  --scsihw virtio-scsi-single \
  --net0 "virtio,bridge=$bridge,firewall=1" \
  --agent enabled=1,fstrim_cloned_disks=1 \
  --onboot 1 \
  --startup order=30,up=30,down=60

qm importdisk "$vmid" "$image_path" "$storage"
disk_volume=$(qm config "$vmid" | awk '/^unused0:/ { print $2 }')
if [ -z "$disk_volume" ]; then
  echo "Imported disk was not registered as unused0." >&2
  exit 5
fi
qm set "$vmid" --scsi0 "$disk_volume,discard=on,iothread=1,ssd=1"
qm disk resize "$vmid" scsi0 200G
qm set "$vmid" \
  --ide2 "$storage:cloudinit" \
  --boot order=scsi0 \
  --serial0 socket \
  --vga serial0 \
  --ciuser deploy \
  --sshkeys "$ssh_key" \
  --ipconfig0 ip=dhcp \
  --nameserver 10.10.14.1 \
  --ciupgrade 1
qm cloudinit update "$vmid"
rm -f "$image_path" "$checksums_path"
qm start "$vmid"

echo "Created and started VMID $vmid. Record this fixed interface configuration:"
qm config "$vmid" | grep -E '^(name|cores|memory|balloon|scsi0|net0|agent|onboot|ipconfig0|startup):'
