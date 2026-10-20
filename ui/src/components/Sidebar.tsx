import { Link, NavLink } from "react-router";
import { useTranslation } from "react-i18next";
import IconHouse from "~icons/ph/house";
import IconHouseFill from "~icons/ph/house-fill";
import IconStack from "~icons/ph/stack";
import IconStackFill from "~icons/ph/stack-fill";
import IconCompass from "~icons/ph/compass";
import IconCompassFill from "~icons/ph/compass-fill";
import IconUser from "~icons/ph/user-circle";
import IconUserFill from "~icons/ph/user-circle-fill";
import IconGear from "~icons/ph/gear-six";
import IconGearFill from "~icons/ph/gear-six-fill";
import type { ComponentType, SVGProps } from "react";
import { SkinFace } from "../features/accounts/SkinFace";
import { useAccounts } from "../features/accounts/useAccounts";

type IconComponent = ComponentType<SVGProps<SVGSVGElement>>;

type Item = {
  to: string;
  key: "home" | "instances" | "browse" | "accounts" | "settings";
  Icon: IconComponent;
  /** 選択中に使う塗りのアイコン。面の明度と文字の強さに加え、形でも現在地を示す */
  ActiveIcon: IconComponent;
};

const ITEMS: Item[] = [
  { to: "/", key: "home", Icon: IconHouse, ActiveIcon: IconHouseFill },
  { to: "/instances", key: "instances", Icon: IconStack, ActiveIcon: IconStackFill },
  { to: "/browse", key: "browse", Icon: IconCompass, ActiveIcon: IconCompassFill },
  { to: "/accounts", key: "accounts", Icon: IconUser, ActiveIcon: IconUserFill },
  { to: "/settings", key: "settings", Icon: IconGear, ActiveIcon: IconGearFill },
];

const itemBase = [
  "flex h-10 items-center gap-2.5 rounded-md px-3",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  // 押した瞬間に面を1段深くする（離すのを待たない）
  "active:bg-surface-3",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
].join(" ");

export function Sidebar({ materialOn }: { materialOn: boolean }) {
  const { t } = useTranslation();
  return (
    <nav
      aria-label="Nexora"
      className={`flex w-56 shrink-0 flex-col gap-1 px-3 pt-2 pb-4 ${materialOn ? "bg-transparent" : "bg-surface-1"}`}
    >
      {ITEMS.map(({ to, key, Icon, ActiveIcon }) => (
        <NavLink
          key={key}
          to={to}
          end={to === "/"}
          className={({ isActive }) =>
            `${itemBase} ${isActive ? "bg-surface-3 text-text-strong" : "text-text-muted hover:bg-surface-2 hover:text-text"}`
          }
        >
          {({ isActive }) => (
            <>
              {isActive ? (
                <ActiveIcon aria-hidden className="size-4 shrink-0" />
              ) : (
                <Icon aria-hidden className="size-4 shrink-0" />
              )}
              <span className="truncate">{t(`nav.${key}`)}</span>
            </>
          )}
        </NavLink>
      ))}
      <div className="mt-auto flex flex-col gap-1">
        <AccountLink />
      </div>
    </nav>
  );
}

/** 使用中のアカウントの顔と名前。押すとアカウント画面に移る（spec 6.2）。 */
function AccountLink() {
  const { t } = useTranslation();
  const { data, active } = useAccounts();
  // 一覧が届くまでは何も出さない（「ログイン」が一瞬見えないように）
  if (!data) return null;
  const needsLogin = active?.kind === "msa" && active.account.needsLogin;
  const name = active?.kind === "msa" ? active.account.username : active?.username;
  return (
    // 現在地はナビゲーションの「アカウント」の項目で示すため、ここは NavLink にしない
    <Link
      to="/accounts"
      // 黄色の点は色だけなので、スクリーンリーダーには名前に続けて「ログインが必要」と伝える
      aria-label={needsLogin ? t("sidebar.accountNeedsLogin", { name }) : undefined}
      className={`${itemBase} text-text-muted hover:bg-surface-2 hover:text-text`}
    >
      {active ? (
        <>
          {/* flex にして、中の inline-block が行ボックスの余白（ベースライン分）を作らないようにする */}
          <span className="relative flex shrink-0">
            <SkinFace accountId={active.kind === "msa" ? active.account.id : null} size={24} />
            {needsLogin && (
              <span aria-hidden className="absolute -top-0.5 -right-0.5 size-2 rounded-full bg-warning" />
            )}
          </span>
          <span className="truncate text-text">{name}</span>
        </>
      ) : (
        <>
          <IconUser aria-hidden className="size-4 shrink-0" />
          <span className="truncate">{t("sidebar.login")}</span>
        </>
      )}
    </Link>
  );
}
