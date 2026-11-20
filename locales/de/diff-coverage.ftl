# Testabdeckung in der Randspalte des Diffs (#173 Punkt 8).

diff-coverage-toggle = Testabdeckung in Diffs: zeigen oder ausblenden
diff-coverage-show = Testabdeckung der geänderten Zeilen zeigen
diff-coverage-hide = Testabdeckung der geänderten Zeilen ausblenden
diff-coverage-mark-covered = Abgedeckt: { $hits }-mal ausgeführt
diff-coverage-mark-uncovered = Nicht abgedeckt: kein Test hat diese Zeile ausgeführt
diff-coverage-mark-unknown = Abdeckung unbekannt: keine aktuellen Belege für diese Zeile
diff-coverage-error = Der Abdeckungsbericht konnte nicht gelesen werden (Details beim Überfahren)
diff-coverage-current = Testabdeckung aus { $report } bei { $sha }
diff-coverage-legend = Grün: abgedeckt. Rot: nicht abgedeckt. Hohl: unbekannt, der Bericht hat keine aktuellen Belege für die Zeile. Geänderte Zeilen ohne Markierung sind nicht ausführbar.
diff-coverage-stale-other = Abdeckung von { $sha }, veraltet: für diese Revision nicht angewendet
diff-coverage-stale-edited = Abdeckung von HEAD ({ $sha }), veraltet: Dateien wurden nach der Messung geändert
diff-coverage-undetermined = Abdeckung von HEAD ({ $sha }): unklar, ob sie die nicht committeten Änderungen enthält, nicht angewendet
diff-coverage-stale-index = Abdeckung des Arbeitsverzeichnisses: die gestagte Fassung dieser Datei weicht davon ab, nicht angewendet
diff-coverage-no-revision = Revision der Abdeckung unbekannt: keine { $report }.sha nennt den getesteten Commit, nicht angewendet
diff-coverage-stale-tip = Es lässt sich nicht zeigen, dass { $report } von { $sha } stammt, der gezeigten Revision; daher ist jede geänderte Zeile als unbekannt markiert. Schreibe nach einem Abdeckungslauf die volle ID des getesteten Commits in { $report }.sha.
diff-coverage-setting-label = Testabdeckung in Diffs
diff-coverage-setting = Geänderte Zeilen als abgedeckt, nicht abgedeckt oder unbekannt markieren
diff-coverage-setting-tip = Liest den Abdeckungsbericht an den üblichen Orten (lcov.info, coverage/lcov.info, coverage.xml, cobertura.xml, *.lcov). Seine Markierungen gelten nur, wenn eine .sha-Datei daneben die Revision nennt, die der Diff zeigt; sonst sind die Zeilen unbekannt und der Diff sagt, warum.
diff-coverage-checking = Der Testabdeckungsbericht wird erneut geprüft…
diff-coverage-checking-tip = Die Markierungen sind unbekannt, bis der Bericht mit der gezeigten Revision abgeglichen ist.
