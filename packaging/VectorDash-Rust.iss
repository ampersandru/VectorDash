[Setup]
AppId={{5ACFE4A2-EC45-4B54-95C7-25B862C57DC9}
AppName=VectorDash Rust Preview
AppVersion=0.3.1
AppPublisher=VectorDash contributors
AppPublisherURL=https://github.com/ampersandru/VectorDash
DefaultDirName={localappdata}\Programs\VectorDashRust
DefaultGroupName=VectorDash Rust Preview
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\dist\installer
OutputBaseFilename=VectorDash-Rust-Setup-0.3.1-preview-x64
SetupIconFile=VectorDash.ico
UninstallDisplayIcon={app}\VectorDash.exe
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
[Files]
Source: "..\dist\VectorDashRust\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{group}\VectorDash Rust Preview"; Filename: "{app}\VectorDash.exe"
Name: "{autodesktop}\VectorDash Rust Preview"; Filename: "{app}\VectorDash.exe"; Tasks: desktopicon
[Run]
Filename: "{app}\VectorDash.exe"; Description: "Open VectorDash Rust Preview"; Flags: nowait postinstall skipifsilent
[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var Command: String;
begin
 if CurUninstallStep = usUninstall then
  if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDashRust', Command) then
   if Pos(Lowercase(ExpandConstant('{app}\VectorDash.exe')), Lowercase(Command)) > 0 then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDashRust');
end;
