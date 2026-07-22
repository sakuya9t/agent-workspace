import { useTranslation } from "react-i18next";

/** Discoverable switch into the bookmarkable `/deck` controller route. */
export function DeckModeLink() {
  const { t } = useTranslation();
  return (
    <a className="deck-mode-link" href="/deck" title={t("deck.openTitle")}>
      <span className="deck-link-icon" aria-hidden="true">
        {Array.from({ length: 6 }, (_, i) => <i key={i} />)}
      </span>
      <span className="sr-only">{t("deck.open")}</span>
    </a>
  );
}
