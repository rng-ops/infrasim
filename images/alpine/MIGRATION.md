# Backwards Compatibility Mapping
# Maps old variant names to new profile compositions

# Old variants -> New profiles
no-vpn: no-vpn-minimal
wireguard: wg-mesh-ipv6
tailscale: ts-managed
dual-vpn: dual-vpn-separated

# Migration notes:
#
# The old `variants/` directory has been replaced with a feature-overlay
# system that provides more flexibility and better composition.
#
# Key differences:
#
# 1. **Modular Features**: Instead of monolithic variant directories,
#    each capability is now a separate feature overlay in `features/`.
#
# 2. **Profile Composition**: Profiles in `profiles/` compose multiple
#    features together with specific configurations.
#
# 3. **IPv6 Rendezvous**: The new default peer discovery uses IPv6
#    epoch/slot-based rendezvous instead of DNS-SD or mDNS. This works
#    across routed networks without requiring multicast.
#
# 4. **Signature Verification**: All peer admission now requires Ed25519
#    signature verification. Peers start with narrow AllowedIPs (/32 or /128)
#    and are only widened after cryptographic verification.
#
# 5. **Selftest Framework**: Each feature includes selftest modules that
#    validate functionality at boot time and can be run on-demand.
#
# 6. **Signed Provenance**: Build outputs include signed provenance
#    documents following the in-toto attestation format.
#
# Migration steps:
#
# 1. Replace `build-variant.sh no-vpn` with:
#    ./compose-profile.sh no-vpn-minimal
#    ./build-profile.sh no-vpn-minimal
#
# 2. Replace `build-variant.sh wireguard` with:
#    ./compose-profile.sh wg-mesh-ipv6
#    ./build-profile.sh wg-mesh-ipv6
#
# 3. Update any cloud-init configurations to include the mesh_secret
#    for IPv6 rendezvous (if using wg-mesh-ipv6 profile).
#
# 4. Deploy trusted signer public keys to /etc/infrasim/trusted-signers/
#    for peer admission verification.
