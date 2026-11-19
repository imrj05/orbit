"""Git page: the opened commit's card (facts, pull requests, checks).

"Commit" is not listed: the existing Git glossary entry already covers it.
"""

_KEYS = [
    "Author", "Committer", "Parents", "GitHub", "Asking GitHub…",
    "GitHub details unavailable", "Not in a pull request", "opened by %{login}",
    "merged by %{login}", "%{count} failed", "%{count} running", "%{count} passed",
    "%{count} skipped", "Open check", "%{count} more checks need a look",
]

_T = {
    "zh-CN": ["作者", "提交者", "父提交", "GitHub", "正在询问 GitHub…",
              "无法获取 GitHub 详情", "不在任何拉取请求中", "由 %{login} 创建",
              "由 %{login} 合并", "%{count} 个失败", "%{count} 个运行中", "%{count} 个通过",
              "%{count} 个跳过", "打开检查", "还有 %{count} 个检查需要查看"],
    "ja": ["作成者", "コミッター", "親", "GitHub", "GitHub に問い合わせ中…",
           "GitHub の詳細を取得できません", "プルリクエストに含まれていません", "%{login} が作成",
           "%{login} がマージ", "%{count} 件失敗", "%{count} 件実行中", "%{count} 件成功",
           "%{count} 件スキップ", "チェックを開く", "確認が必要なチェックがあと %{count} 件"],
    "ko": ["작성자", "커미터", "부모", "GitHub", "GitHub에 묻는 중…",
           "GitHub 세부 정보를 가져올 수 없음", "풀 리퀘스트에 포함되지 않음", "%{login} 님이 생성",
           "%{login} 님이 병합", "%{count}개 실패", "%{count}개 실행 중", "%{count}개 통과",
           "%{count}개 건너뜀", "검사 열기", "확인이 필요한 검사 %{count}개 더"],
    "es": ["Autor", "Confirmador", "Padres", "GitHub", "Consultando a GitHub…",
           "Detalles de GitHub no disponibles", "No está en ninguna pull request", "abierta por %{login}",
           "fusionada por %{login}", "%{count} fallidas", "%{count} en curso", "%{count} correctas",
           "%{count} omitidas", "Abrir comprobación", "%{count} comprobaciones más requieren atención"],
    "fr": ["Auteur", "Committer", "Parents", "GitHub", "Interrogation de GitHub…",
           "Détails GitHub indisponibles", "Dans aucune pull request", "ouverte par %{login}",
           "fusionnée par %{login}", "%{count} en échec", "%{count} en cours", "%{count} réussies",
           "%{count} ignorées", "Ouvrir la vérification", "%{count} autres vérifications à examiner"],
    "de": ["Autor", "Committer", "Eltern", "GitHub", "GitHub wird abgefragt…",
           "GitHub-Details nicht verfügbar", "In keinem Pull Request", "geöffnet von %{login}",
           "gemergt von %{login}", "%{count} fehlgeschlagen", "%{count} laufen", "%{count} bestanden",
           "%{count} übersprungen", "Prüfung öffnen", "%{count} weitere Prüfungen brauchen Aufmerksamkeit"],
    "pt-BR": ["Autor", "Committer", "Pais", "GitHub", "Consultando o GitHub…",
              "Detalhes do GitHub indisponíveis", "Não está em nenhum pull request", "aberto por %{login}",
              "mesclado por %{login}", "%{count} falharam", "%{count} em execução", "%{count} passaram",
              "%{count} ignoradas", "Abrir verificação", "Mais %{count} verificações precisam de atenção"],
    "ru": ["Автор", "Коммитер", "Родители", "GitHub", "Запрос к GitHub…",
           "Сведения GitHub недоступны", "Не входит в pull request", "открыл %{login}",
           "слил %{login}", "не пройдено: %{count}", "выполняется: %{count}", "пройдено: %{count}",
           "пропущено: %{count}", "Открыть проверку", "Ещё проверок требуют внимания: %{count}"],
    "it": ["Autore", "Committer", "Genitori", "GitHub", "Interrogazione di GitHub…",
           "Dettagli GitHub non disponibili", "Non è in nessuna pull request", "aperta da %{login}",
           "unita da %{login}", "%{count} non riusciti", "%{count} in corso", "%{count} superati",
           "%{count} saltati", "Apri controllo", "Altri %{count} controlli da verificare"],
}

GLOSSARY = {locale: dict(zip(_KEYS, values)) for locale, values in _T.items()}
assert all(len(values) == len(_KEYS) for values in _T.values())
