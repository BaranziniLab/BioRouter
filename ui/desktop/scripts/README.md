# BioRouter Scripts

Put `biorouter` in your $PATH if you want to launch via: `biorouter .`

```
biorouter .
```

This will open BioRouter GUI from any path you specify

# Unregister Deeplink Protocols (macos only)

`unregister-deeplink-protocols.js` is a script to unregister the deeplink protocol used by BioRouter like `biorouter://`.
This is handy when you want to test deeplinks with the development version of BioRouter.

# Usage

To unregister the deeplink protocols, run the following command in your terminal:
Then launch BioRouter again and your deeplinks should work from the latest launched BioRouter application as it is registered on startup.

```bash
node scripts/unregister-deeplink-protocols.js
```

