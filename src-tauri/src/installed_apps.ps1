$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$apps = [System.Collections.Generic.List[object]]::new()
$warnings = [System.Collections.Generic.List[string]]::new()
$uninstall = 'Software\Microsoft\Windows\CurrentVersion\Uninstall'

foreach ($hive in @('LocalMachine', 'CurrentUser')) {
    foreach ($view in @('Registry64', 'Registry32')) {
        try {
            $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($hive, $view)
            try {
                $root = $base.OpenSubKey($uninstall)
                if ($null -eq $root) { continue }
                try {
                    foreach ($key in $root.GetSubKeyNames()) {
                        try {
                            $entry = $root.OpenSubKey($key)
                            if ($null -eq $entry) { continue }
                            try {
                                $name = [string]$entry.GetValue('DisplayName')
                                if (!$name.Trim() -or $entry.GetValue('SystemComponent') -eq 1 -or $entry.GetValue('ParentKeyName')) { continue }
                                $location = [string]$entry.GetValue('InstallLocation')
                                $apps.Add(@{
                                    name = $name.Trim()
                                    version = [string]$entry.GetValue('DisplayVersion')
                                    publisher = [string]$entry.GetValue('Publisher')
                                    location = $location.Trim().Trim('"')
                                    source = 'unknown'
                                    registration = 'desktop'
                                })
                            } finally { $entry.Dispose() }
                        } catch {
                            if (!$warnings.Contains('Some desktop application entries could not be read.')) {
                                $warnings.Add('Some desktop application entries could not be read.')
                            }
                        }
                    }
                } finally { $root.Dispose() }
            } finally { $base.Dispose() }
        } catch {
            $warnings.Add("Could not read installed apps from $hive ($view).")
        }
    }
}

try {
    $packages = @(Get-AppxPackage -PackageTypeFilter Main)
    foreach ($package in $packages) {
        $name = [string]$package.Name
        $publisher = [string]$package.Publisher
        try {
            $manifest = Get-AppxPackageManifest -Package $package.PackageFullName
            $displayName = [string]$manifest.Package.Properties.DisplayName
            $displayPublisher = [string]$manifest.Package.Properties.PublisherDisplayName
            if ($displayName -and $displayName -notlike 'ms-resource:*') { $name = $displayName }
            if ($displayPublisher -and $displayPublisher -notlike 'ms-resource:*') { $publisher = $displayPublisher }
        } catch {
            # The package identity remains available when manifest access fails.
        }
        $source = if ($package.SignatureKind -eq 'Store') { 'storeSigned' } else { 'unknown' }
        $apps.Add(@{
            name = $name
            version = [string]$package.Version
            publisher = $publisher
            location = [string]$package.InstallLocation
            source = $source
            registration = 'msix'
        })
    }
} catch {
    $warnings.Add('Could not read packaged apps for the current Windows user.')
}

@{ apps = $apps.ToArray(); warnings = $warnings.ToArray() } | ConvertTo-Json -Depth 4 -Compress
