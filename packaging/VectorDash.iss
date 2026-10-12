[Setup]
AppId={{C6AC1B4D-A119-4D73-B2C8-6C507DEFE923}
AppName=VectorDash
AppVersion=0.2.21
AppPublisher=VectorDash contributors
AppPublisherURL=https://github.com/ampersandru/VectorDash
DefaultDirName={localappdata}\Programs\VectorDash
DefaultGroupName=VectorDash
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\dist\installer
OutputBaseFilename=VectorDash-Setup-0.2.21-x64
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
Source: "..\dist\VectorDash\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{group}\VectorDash"; Filename: "{app}\VectorDash.exe"
Name: "{autodesktop}\VectorDash"; Filename: "{app}\VectorDash.exe"; Tasks: desktopicon
[Run]
Filename: "{app}\VectorDash.exe"; Description: "Open VectorDash"; Flags: nowait postinstall skipifsilent
[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var Command: String;
begin
  if CurUninstallStep = usUninstall then
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDash', Command) then
      if Pos(Lowercase(ExpandConstant('{app}\VectorDash.exe')), Lowercase(Command)) > 0 then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDash');
end;
