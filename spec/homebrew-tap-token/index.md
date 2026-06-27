# HOMEBREW_TAP_TOKEN

⏺ HOMEBREW_TAP_TOKEN is a GitHub Personal Access Token (PAT) that the release workflow uses to push the formula into vixide/homebrew-tap (the built-in GITHUB_TOKEN
  can't write to a different repo, which is why a separate token is needed). Two parts: create the token, then add it as a secret.

  1. Create the token

  Option A — Fine-grained PAT (recommended, least privilege)
  1. GitHub → your avatar → Settings → Developer settings → Personal access tokens → Fine-grained tokens → Generate new token.
  (Direct: https://github.com/settings/personal-access-tokens/new)
  2. Resource owner: vixide (the org). (If vixide isn't selectable, the org must allow fine-grained PATs, or use Option B.)
  3. Repository access: Only select repositories → pick vixide/homebrew-tap.
  4. Permissions → Repository permissions → Contents: Read and write (this also auto-enables Metadata: read).
  5. Set an expiration, Generate token, and copy it (shown once).

  Option B — Classic PAT (simpler, broader)
  1. Settings → Developer settings → Tokens (classic) → Generate new token (classic).
  (Direct: https://github.com/settings/tokens/new)
  2. Check the repo scope.
  3. Generate and copy. (If the tap repo is owned by the vixide org and has SSO enabled, click Configure SSO on the token and authorize vixide.)

  2. Add it as a secret on vixide/vix

  1. Go to github.com/vixide/vix → Settings → Secrets and variables → Actions.
  2. New repository secret → Name: HOMEBREW_TAP_TOKEN → Value: paste the token → Add secret.

  Or via CLI once gh is authed:
  gh secret set HOMEBREW_TAP_TOKEN --repo vixide/vix
  (it'll prompt you to paste the token).