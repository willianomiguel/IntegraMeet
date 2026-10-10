; Hooks do instalador NSIS (Tauri).
; Regra: binários, cache e logs saem sempre. Modelos de IA e reuniões
; só saem se o usuário confirmar, e o padrão é MANTER os dados.

!macro NSIS_HOOK_POSTUNINSTALL
  ; Cache e logs não são dados do usuário.
  RMDir /r "$LOCALAPPDATA\LocalMeetingAI\cache"
  RMDir /r "$LOCALAPPDATA\LocalMeetingAI\logs"

  ; Em desinstalação silenciosa, nada além de cache e logs é removido.
  IfSilent fim_dados

  ; Pergunta 1: modelos de IA (ocupam vários GB e podem ser baixados de novo).
  MessageBox MB_YESNO|MB_ICONQUESTION "Deseja remover os modelos de IA baixados? Eles ocupam vários gigabytes e podem ser baixados novamente depois. Escolha Não para mantê-los." IDNO pula_modelos
    RMDir /r "$LOCALAPPDATA\LocalMeetingAI\models"
  pula_modelos:

  ; Pergunta 2: reuniões, transcrições e análises. Não há como recuperar depois.
  MessageBox MB_YESNO|MB_ICONEXCLAMATION "Deseja apagar TODAS as suas reuniões, gravações, transcrições e análises? Esta ação não pode ser desfeita. Escolha Não para mantê-las." IDNO pula_reunioes
    RMDir /r "$APPDATA\LocalMeetingAI\data"
    RMDir /r "$DOCUMENTS\LocalMeetingAI"
  pula_reunioes:

  fim_dados:
!macroend
