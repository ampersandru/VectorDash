[Setup]
AppId={{C6AC1B4D-A119-4D73-B2C8-6C507DEFE923}
AppName=VectorDash
AppVerName=VectorDash
UninstallDisplayName=VectorDash
AppVersion=0.3.25-beta.5
VersionInfoVersion=0.3.25.0
AppPublisher=VectorDash contributors
AppPublisherURL=https://github.com/ampersandru/VectorDash
DefaultDirName={localappdata}\Programs\VectorDash
DefaultGroupName=VectorDash
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir=..\dist\installer
OutputBaseFilename=VectorDash-Beta-Setup-0.3.25-beta.5-x64
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
[InstallDelete]
; Only the old bundled Python runtime; user data lives outside this directory.
Type: filesandordirs; Name: "{app}\_internal"
[Icons]
Name: "{group}\VectorDash"; Filename: "{app}\VectorDash.exe"
Name: "{autodesktop}\VectorDash"; Filename: "{app}\VectorDash.exe"; Tasks: desktopicon
[Run]
Filename: "{app}\VectorDash.exe"; Description: "Open VectorDash"; Flags: nowait postinstall skipifsilent
[Code]
procedure CurStepChanged(CurStep: TSetupStep);
var OldUninstaller, OldFolder, StartupCommand: String; ResultCode: Integer; HadPreviewStartup: Boolean;
begin
 if CurStep = ssPostInstall then
  if RegQueryStringValue(HKCU,
   'Software\Microsoft\Windows\CurrentVersion\Uninstall\{5ACFE4A2-EC45-4B54-95C7-25B862C57DC9}_is1',
   'UninstallString', OldUninstaller) then
  begin
   OldUninstaller := RemoveQuotes(OldUninstaller);
   OldFolder := ExtractFileDir(OldUninstaller);
   HadPreviewStartup := RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run',
      'VectorDashRust', StartupCommand);
   if HadPreviewStartup then
    HadPreviewStartup := Pos(Lowercase(OldFolder + '\VectorDash.exe'), Lowercase(StartupCommand)) > 0;
   { Never uninstall the newly upgraded application or touch its data folders. }
   if (CompareText(OldFolder, RemoveBackslashUnlessRoot(ExpandConstant('{app}'))) <> 0)
    and FileExists(OldUninstaller) then
    if not Exec(OldUninstaller, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
      Log('Old preview removal did not complete; uninstall it from Installed apps.');
   if HadPreviewStartup then
    RegWriteStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run',
     'VectorDash', '"' + ExpandConstant('{app}\VectorDash.exe') + '" --startup');
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var Command: String;
begin
 if CurUninstallStep = usUninstall then
  if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDashRust', Command) then
   if Pos(Lowercase(ExpandConstant('{app}\VectorDash.exe')), Lowercase(Command)) > 0 then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDashRust');
 if CurUninstallStep = usUninstall then
  if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDash', Command) then
   if Pos(Lowercase(ExpandConstant('{app}\VectorDash.exe')), Lowercase(Command)) > 0 then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'VectorDash');
end;
