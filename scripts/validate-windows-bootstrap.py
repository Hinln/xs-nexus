#!/usr/bin/env python3

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
BOOTSTRAP = ROOT / "installers" / "windows" / "xs-nexus-one-click.ps1"


def main() -> int:
    try:
        text = BOOTSTRAP.read_text(encoding="utf-8")
        required = (
            "__RELEASE_MANIFEST_SHA256__",
            "Assert-True ([Environment]::Is64BitOperatingSystem)",
            "$manifest.archive_size -is [int] -or $manifest.archive_size -is [long]",
            "$archiveSize = [long] $manifest.archive_size",
            "$UpdateSigningPublicKeyUrl",
            "$ExpectedUpdateSigningPublicKeySha256",
            "update signing public key verification failed",
            "-MaximumRedirection 0",
            "'refusing to change ACL on a reparse point'",
            "function Set-AgentPrivateFileAcl",
            "'D:P(A;;FA;;;SY)(A;;FA;;;BA)'",
            "function Set-AgentPrivateDirectoryAcl",
            "'D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)'",
            "$pinnedUpdateSigningPublicKeyPath",
            "[IO.File]::WriteAllText($tokenPath, $value, [Text.UTF8Encoding]::new($false))",
            "Set-AgentPrivateFileAcl $tokenPath",
            "$createdService = $false",
            "if ($createdService -and (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue))",
            "(@($actualPayload | Sort-Object) -join \"`n\") -eq",
        )
        for value in required:
            if value not in text:
                raise AssertionError(f"missing required bootstrap guard: {value}")
        if text.count("__RELEASE_MANIFEST_SHA256__") != 1:
            raise AssertionError("bootstrap manifest marker must appear exactly once")
        if "Assert-True [Environment]::Is64BitOperatingSystem" in text:
            raise AssertionError("unparenthesized PowerShell architecture assertion is unsafe")
        if "Assert-True (($actualPayload | Sort-Object) -join" in text:
            raise AssertionError("payload comparison must group both joined strings before -eq")
        if "Set-RestrictedAcl $temporaryRoot" in text:
            raise AssertionError("temporary token directory must use the Agent private directory ACL")
        if re.search(r"(?m)^\s*Assert-True\s+(?!\()", text):
            raise AssertionError("every Assert-True condition must be parenthesized")
    except (AssertionError, OSError) as error:
        print(f"Windows one-click bootstrap validation failed: {error}", file=sys.stderr)
        return 1
    print("Windows one-click bootstrap source invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
