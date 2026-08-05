---
name: _template-molecule-rustcopy
version: 1.0.0
category: molecule-template
parent: rustcopy-flow
description: "Template per creare nuove molecole per rustcopy-flow. Max 8 step, nessuna dipendenza da tool MCP — solo Bash/PowerShell sul binario rustcopy."
---

# Template Molecola Rustcopy

## Istruzioni
1. Copia in `molecules/molecule-{N}-{nome}.md`
2. Compila il frontmatter
3. Ogni step: comando shell (Bash/PowerShell) o decisione, input, output, criterio di successo
4. Max 8 step — oltre, dividi in 2 molecole
5. Failure Modes obbligatori — includi sempre cosa fare se rustcopy ritorna un exit code
   inatteso, non solo il caso felice
6. Se lo step tocca un'operazione distruttiva (purge, sovrascrittura, cancellazione), il
   checkpoint umano è OBBLIGATORIO e non delegabile a un "procedi" generico dato in precedenza

---

# Molecola: {Nome}

## Input
- {file/parametro di input dalla fase precedente}

## Steps

1. **{comando o decisione}** — {cosa fa in 1 frase}
   - Comando: `{bin} {flag...}` (se applicabile)
   - Input: {parametri}
   - Output: {file o stato prodotto}
   - Output metric: {criterio di successo}

2. ...

## Output Finale
- {file1}
- {file2}

## Failure Modes
- **{errore}**: {recovery}
