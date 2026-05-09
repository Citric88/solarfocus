# Code signing setup (v2.1+)

The `release.yml` workflow signs binaries on tag push **only when the
appropriate secrets are set**. Without secrets the workflow runs
clean and produces unsigned binaries with a clear `::notice::` log
explaining why. This means there's zero pressure to buy certs before
v2.1 ships — you can enable signing on any future release just by
adding the secrets.

## macOS — Developer ID Application

### What it costs

Apple Developer Program: **$99/year**. Includes one Developer ID
certificate for distributing outside the App Store. Required to
avoid the "unidentified developer" Gatekeeper warning.

### Generating the cert

1. Enroll at https://developer.apple.com/programs/.
2. Visit https://developer.apple.com/account/resources/certificates →
   Certificates → "+" → **Developer ID Application**.
3. Generate a CSR via Keychain Access → Certificate Assistant →
   Request a Certificate from a CA. Save to disk.
4. Upload the CSR to the developer portal. Download the resulting
   `.cer` file.
5. Double-click the `.cer` to import to Keychain. Right-click in
   Keychain → **Export** → `.p12` format with a strong password.

### Configuring GitHub secrets

Run locally:

```bash
base64 -i developer-id.p12 | pbcopy
```

Then in GitHub repo → Settings → Secrets and variables → Actions:

| Secret name | Value |
|---|---|
| `MACOS_DEVELOPER_ID_P12_BASE64` | base64 of the `.p12` file |
| `MACOS_DEVELOPER_ID_P12_PASSWORD` | password you set during export |
| `MACOS_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` exactly |

Test by pushing a tag: `git tag -a v2.1.0 -m "test" && git push origin v2.1.0`.
Watch the `macos-release` job; the "Sign binary" step should run and
verify cleanly.

### Notarization (optional but recommended)

Even with signing, macOS 11+ requires notarization to remove the
"this app was downloaded from the Internet" prompt. That's a
separate step using `xcrun notarytool` with an app-specific
password from the Developer portal. The current workflow does NOT
notarize; that's a follow-up because notary turnaround can be
~10–30 min and would block the release pipeline. v2.2 will likely
add it as an async post-release step.

## Windows — Authenticode

### What it costs

Standard Authenticode cert: **$200–500/year** (Sectigo, DigiCert,
SSL.com, etc.). EV cert: **~$500/year** (faster SmartScreen
reputation). For a side project the standard cert is fine — it
just takes a few weeks to accumulate Reputation™ for SmartScreen
to stop warning.

Cheaper alternative: **Azure Trusted Signing** (~$120/year) —
Microsoft's managed signing service. Simpler workflow, no PFX
file, signs via Azure AD identity. The `release.yml` workflow
currently uses the classic PFX path; switching to Trusted
Signing would replace the `signtool.exe` invocation with `azuresigntool`
and use `AZURE_CLIENT_ID` / `AZURE_TENANT_ID` / `AZURE_CLIENT_SECRET`
secrets instead.

### Generating the cert (PFX path)

After purchase the CA gives you a `.pfx` file directly (or a CSR
process similar to macOS). Save it locally with a strong password.

### Configuring GitHub secrets

```bash
base64 -i authenticode.pfx | tr -d '\n' | pbcopy
```

In GitHub:

| Secret name | Value |
|---|---|
| `WINDOWS_AUTHENTICODE_PFX_BASE64` | base64 of the `.pfx` |
| `WINDOWS_AUTHENTICODE_PFX_PASSWORD` | password |

The workflow uses DigiCert's free public timestamp server
(`http://timestamp.digicert.com`) so signatures stay valid past
the cert expiration.

## Verifying a signed binary

### macOS

```bash
codesign --verify --deep --strict --verbose=2 solarfocus-desktop
spctl --assess --type execute --verbose solarfocus-desktop
```

Both should succeed with "valid on disk" / "accepted".

### Windows

```powershell
Get-AuthenticodeSignature .\solarfocus-desktop.exe
```

`Status` should be `Valid`. SignerCertificate should match your
purchased cert.

## What's NOT covered

- **Hardened runtime entitlements** — required for notarization but
  the current build doesn't need any beyond defaults
  (no JIT, no DYLD env vars, no debug attach).
- **Windows installer signing** — when v2.3 adds an MSI, the
  installer itself will also need signing (same cert, same secrets).
- **Apple Distribution / App Store** — out of scope; we ship outside
  the App Store directly.
- **Cert rotation playbook** — when the cert expires, the existing
  workflow keeps signing with the old key (and timestamps mean old
  releases stay valid), but new releases will fail. Update the
  secrets, push a new tag.

## Cost summary

If signing is enabled for v2.1:
- macOS: $99/year (Apple Developer Program)
- Windows: $120/year (Azure Trusted Signing) OR $200+/year (PFX cert)

Total: **~$220–300/year** for both platforms. Before that, binaries
ship unsigned and users see Gatekeeper / SmartScreen warnings on
first run — annoying but not blocking, and clearly documented in
the README.
