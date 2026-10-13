// Is the console still open? (14 Ott 2026)
//
// Included into the [Code] section of rustcopy.iss, and into a throwaway test installer when this logic
// is exercised without admin rights (same arrangement as install-report.pas).
//
// Why this exists. Closing the console's window did not close the program: it stayed in the notification
// area, holding rustcopy-gui.exe open, so an upgrade could not replace it. Setup's own RestartManager
// asked it to close, waited about thirty seconds, then offered Abort/Retry/Ignore with a message that said
// nothing about where the program was or how to close it. This asks first, in words a person can act on.
//
// How it knows. The console takes a named mutex as its single-instance lock (crates/rustcopy-ui,
// single_instance.rs: "Local\rustcopy-ui", the same tag the pipe uses). CheckForMutexes looks for it without
// starting any process -- no tasklist, no PowerShell: an unsigned installer launching a shell to look at the
// process list is what an EDR on a server blocks (see install-report.pas). "Local\" is the current session's
// namespace, which is where an elevated Setup started by the same person lives.

const
  CONSOLE_MUTEX = 'Local\rustcopy-ui';

function IsConsoleRunning(): Boolean;
begin
  Result := CheckForMutexes(CONSOLE_MUTEX);
end;

// The question put to the person: where the program may be, and two ways to close it.
function ConsoleStillOpenMessage(): string;
begin
  Result :=
    'rustcopy e'' ancora aperto. Anche se non vedi la finestra, puo'' essere rimasto in esecuzione ' +
    'vicino all''orologio (icona di rustcopy, a volte dentro la freccia delle icone nascoste).' + #13#10 + #13#10 +
    'Per chiuderlo:' + #13#10 +
    '  - clic destro sull''icona di rustcopy, poi "Esci";' + #13#10 +
    '  - oppure Gestione attivita'' (Ctrl+Maiusc+Esc), cerca "rustcopy", clic destro, "Termina attivita''".' + #13#10 + #13#10 +
    'Poi premi "Riprova". Se una copia e'' in corso, aspetta che finisca.';
end;

// Returns True when it is fine to go on: the console is closed, or the install is silent (nobody to ask --
// Setup's own mechanism then does what it always did). Returns False when the person gave up.
function WaitForConsoleToClose(): Boolean;
begin
  Result := True;
  while IsConsoleRunning() do
  begin
    ReportAdd('Console ancora aperta: chiedo di chiuderla');
    if WizardSilent() then
    begin
      ReportAdd('Installazione silenziosa: proseguo senza chiedere');
      exit;
    end;
    if SuppressibleMsgBox(ConsoleStillOpenMessage(), mbConfirmation, MB_RETRYCANCEL, IDRETRY) <> IDRETRY then
    begin
      ReportAdd('Annullata dalla persona: la console era ancora aperta');
      Result := False;
      exit;
    end;
  end;
  ReportAdd('Console chiusa: proseguo');
end;
