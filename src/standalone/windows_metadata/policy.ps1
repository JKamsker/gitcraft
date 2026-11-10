
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$PSModuleAutoloadingPreference = 'None'
try {
  $source = [Environment]::GetEnvironmentVariable('GITCRAFT_ACL_SOURCE')
  $target = [Environment]::GetEnvironmentVariable('GITCRAFT_ACL_TARGET')
  $snapshot = [Environment]::GetEnvironmentVariable('GITCRAFT_ACL_SNAPSHOT') -eq '1'
  if ([String]::IsNullOrEmpty($source) -or (-not $snapshot -and [String]::IsNullOrEmpty($target))) { throw 'Missing metadata paths' }
  Microsoft.PowerShell.Core\Import-Module ([IO.Path]::Combine($PSHOME, 'Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')) -ErrorAction Stop
  Microsoft.PowerShell.Management\Get-Item -LiteralPath $source -Stream '*' -ErrorAction Stop | Microsoft.PowerShell.Core\ForEach-Object {
    if ($_.Stream -ne ':$DATA' -and $_.Stream -ne '$DATA' -and $_.Stream -ne '::$DATA') { throw 'Named data streams cannot be preserved with the bounded save protocol' }
  }
  # Try the full descriptor, but normal users cannot request the audit SACL.
  # Only ERROR_PRIVILEGE_NOT_HELD permits an owner/group/DACL-only retry.
  Microsoft.PowerShell.Core\Import-Module ([IO.Path]::Combine($PSHOME, 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')) -ErrorAction Stop
  Microsoft.PowerShell.Utility\Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
public static class GitcraftNativePolicy {
  // OWNER|GROUP|DACL|SACL|LABEL|ATTRIBUTE|SCOPE|PROCESS_TRUST_LABEL|ACCESS_FILTER.
  public const uint AllPolicy = 0x000001ff;
  public const uint BasicPolicy = 0x00000007; // OWNER|GROUP|DACL; always required.
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode)]
  static extern uint GetNamedSecurityInfo(string path, uint type, uint info, out IntPtr owner, out IntPtr group, out IntPtr dacl, out IntPtr sacl, out IntPtr descriptor);
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode)]
  static extern uint SetNamedSecurityInfo(string path, uint type, uint info, IntPtr owner, IntPtr group, IntPtr dacl, IntPtr sacl);
  [DllImport("advapi32.dll")] static extern uint GetSecurityDescriptorLength(IntPtr descriptor);
  [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
  static void Check(uint error) {
    if (error != 0) throw new InvalidOperationException("Full Windows policy unavailable (native error " + error + "); SACL/audit access may require SeSecurityPrivilege. Original must remain unchanged.");
  }
  static byte[] Canonical(IntPtr descriptor) {
    uint length = GetSecurityDescriptorLength(descriptor);
    if (length == 0 || length > 1048576) throw new InvalidOperationException("Full native descriptor exceeds 1 MiB");
    byte[] raw = new byte[length]; Marshal.Copy(descriptor,raw,0,(int)length);
    RawSecurityDescriptor parsed = new RawSecurityDescriptor(raw,0);
    // The resource manager recalculates these status bits when applying an otherwise
    // identical ACL. Keep ACEs and the protected/unprotected state in the comparison.
    const ControlFlags InheritanceStatus = ControlFlags.DiscretionaryAclAutoInheritRequired |
      ControlFlags.SystemAclAutoInheritRequired | ControlFlags.DiscretionaryAclAutoInherited |
      ControlFlags.SystemAclAutoInherited;
    parsed.SetFlags(parsed.ControlFlags & ~InheritanceStatus);
    byte[] normalized = new byte[parsed.BinaryLength]; parsed.GetBinaryForm(normalized,0);
    return normalized;
  }
  static void GetPolicy(string path, out uint info, out IntPtr owner, out IntPtr group, out IntPtr dacl, out IntPtr sacl, out IntPtr descriptor) {
    info = AllPolicy;
    uint error = GetNamedSecurityInfo(path,1,info,out owner,out group,out dacl,out sacl,out descriptor);
    if (error == 1314) { // ERROR_PRIVILEGE_NOT_HELD, never an unreadable DACL.
      if (descriptor != IntPtr.Zero) { LocalFree(descriptor); descriptor=IntPtr.Zero; }
      info = BasicPolicy;
      error = GetNamedSecurityInfo(path,1,info,out owner,out group,out dacl,out sacl,out descriptor);
    }
    Check(error);
  }
  static byte[] Snapshot(IntPtr descriptor, uint info) {
    byte[] raw = Canonical(descriptor);
    // Bind comparisons to the captured scope: omitted SACL is not an empty SACL.
    byte[] snapshot = new byte[raw.Length+4];
    Array.Copy(BitConverter.GetBytes(info),snapshot,4);
    Array.Copy(raw,0,snapshot,4,raw.Length);
    return snapshot;
  }
  public static byte[] Read(string path) {
    IntPtr owner,group,dacl,sacl,descriptor=IntPtr.Zero;
    uint info;
    try { GetPolicy(path,out info,out owner,out group,out dacl,out sacl,out descriptor); return Snapshot(descriptor,info); }
    finally { if (descriptor != IntPtr.Zero) LocalFree(descriptor); }
  }
  public static void Copy(string source,string target) {
    IntPtr owner,group,dacl,sacl,descriptor=IntPtr.Zero;
    try {
      uint info;
      GetPolicy(source,out info,out owner,out group,out dacl,out sacl,out descriptor);
      byte[] before=Snapshot(descriptor,info);
      RawSecurityDescriptor parsed=new RawSecurityDescriptor(Canonical(descriptor),0);
      uint protection = (parsed.ControlFlags & ControlFlags.DiscretionaryAclProtected) != 0 ? 0x80000000u : 0x20000000u;
      // SACL protection flags also require SeSecurityPrivilege. Never set them
      // when only the basic policy was captured, or pass an unrequested SACL.
      if ((info & 0x8) != 0) protection |= (parsed.ControlFlags & ControlFlags.SystemAclProtected) != 0 ? 0x40000000u : 0x10000000u;
      Check(SetNamedSecurityInfo(target,1,info|protection,owner,group,dacl,(info & 0x8) != 0 ? sacl : IntPtr.Zero));
      if (!Convert.ToBase64String(before).Equals(Convert.ToBase64String(Read(target)),StringComparison.Ordinal)) throw new InvalidOperationException("Full native policy differs after copy");
      if (!Convert.ToBase64String(before).Equals(Convert.ToBase64String(Read(source)),StringComparison.Ordinal)) throw new InvalidOperationException("Source policy changed during copy");
    } finally { if (descriptor != IntPtr.Zero) LocalFree(descriptor); }
  }
}
'@ -ErrorAction Stop
  $descriptor = [GitcraftNativePolicy]::Read($source)
  if ($snapshot) {
    [Console]::Out.WriteLine([Convert]::ToBase64String($descriptor))
    exit 0
  }
  [GitcraftNativePolicy]::Copy($source, $target)
  [Console]::Out.WriteLine('gitcraft-acl-v1-ok')
  exit 0
} catch {
  [Console]::Error.WriteLine('Could not preserve bounded Windows access metadata: ' + $_.Exception.Message)
  exit 1
}
