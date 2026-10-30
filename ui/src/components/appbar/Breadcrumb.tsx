import { useTranslation } from "react-i18next";
import { Link, useLocation } from "react-router";
import IconCaretRight from "~icons/ph/caret-right";
import { useInstances } from "../../features/instances/useInstances";
import { crumbsFor } from "../../features/nav/crumbs";

/** 今の場所。最後以外はリンクで、親の画面へ戻れる。長い名前は 1 つの要素だけが省略記号で縮む。 */
export function Breadcrumb() {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const { data: instances } = useInstances();
  const id = /^\/instances\/([^/]+)/.exec(pathname)?.[1];
  const instanceName = instances?.find((instance) => instance.id === id)?.name ?? null;
  const crumbs = crumbsFor(pathname, { instanceName, t });
  return (
    <nav aria-label={t("appBar.breadcrumb")} data-testid="page-name" className="ml-2 min-w-0 text-sm">
      <ol className="flex min-w-0 items-center gap-1">
        {crumbs.map((crumb, i) => {
          const last = i === crumbs.length - 1;
          // 縮んでよいのは 1 つだけ。3 つ以上なら間の要素（インスタンス名）、2 つなら最後の要素。
          // 先頭と、間があるときの最後の要素は縮まず、窓の操作ボタンも押し出されない。
          const shrinks = crumbs.length > 2 ? i > 0 && !last : last;
          return (
            <li key={i} className={`flex items-center gap-1 ${shrinks ? "min-w-0" : "shrink-0"}`}>
              {i > 0 && <IconCaretRight aria-hidden className="size-3 shrink-0 text-text-faint" />}
              {crumb.to === null ? (
                <span aria-current="page" className="truncate text-text">
                  {crumb.label}
                </span>
              ) : (
                <Link
                  to={crumb.to}
                  className="min-w-0 truncate rounded-sm text-text-muted transition-colors duration-(--duration-fast) ease-out-quint hover:text-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                >
                  {crumb.label}
                </Link>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
