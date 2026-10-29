import { useEffect, type ReactNode } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check-circle-fill";
import IconSpinner from "~icons/ph/circle-notch";
import IconNone from "~icons/ph/prohibit";
import { useCurrentSkin, useSkinActions } from "../../features/skins/useSkins";
import { useToast } from "../toast/useToast";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const chip = [
  "absolute inset-0 flex items-center justify-center overflow-hidden rounded-md border-2 bg-surface-1",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 pressed:bg-surface-3",
  ring,
].join(" ");

// マントの正面（64x32 の画像の x=1,y=1 から 10x16）だけを切り出す
const FRONT = { x: 1, y: 1, w: 10, h: 16 };

function CapeFace({ image }: { image: string }) {
  return (
    <span
      aria-hidden
      className="absolute inset-1.5 rounded-sm"
      style={{
        backgroundImage: `url("${image}")`,
        backgroundSize: `${(64 / FRONT.w) * 100}% ${(32 / FRONT.h) * 100}%`,
        backgroundPosition: `${(FRONT.x / (64 - FRONT.w)) * 100}% ${(FRONT.y / (32 - FRONT.h)) * 100}%`,
        backgroundRepeat: "no-repeat",
        imageRendering: "pixelated",
      }}
    />
  );
}

function CapeChip({ label, selected, pending, disabled, onPress, children }: {
  label: string;
  selected: boolean;
  pending: boolean;
  disabled: boolean;
  onPress: () => void;
  children: ReactNode;
}) {
  return (
    <div aria-busy={pending || undefined} className="relative h-20 w-[50px] shrink-0">
      <Button
        aria-label={label}
        aria-pressed={selected}
        isDisabled={disabled}
        onPress={onPress}
        className={`${chip} ${selected ? "border-accent" : "border-line hover:border-line-strong"}`}
      >
        {children}
      </Button>
      {selected ? (
        <IconCheck aria-hidden data-testid="cape-check" className="pointer-events-none absolute top-0.5 right-0.5 size-4 text-accent" />
      ) : pending ? (
        <IconSpinner aria-hidden className="pointer-events-none absolute top-0.5 right-0.5 size-4 text-text-muted motion-safe:animate-spin" />
      ) : null}
    </div>
  );
}

/**
 * 持っているマントの札と「外す」の札。マントが 0、適用できないアカウント、取得の失敗では何も出さない。
 * 押した札は、取り直しが終わるまで印を古いままにして進行中の印を出す。
 * blocked は保存したスキンなど、ほかの操作の最中。
 */
export function CapePicker({ accountId, canApply, blocked = false, onBusyChange }: {
  accountId: string | null;
  canApply: boolean;
  blocked?: boolean;
  onBusyChange?: (busy: boolean) => void;
}) {
  const { t } = useTranslation();
  const current = useCurrentSkin(canApply ? accountId : null);
  const actions = useSkinActions(accountId);
  const toast = useToast();

  const busy = actions.busy;
  useEffect(() => {
    onBusyChange?.(busy);
    return () => onBusyChange?.(false);
  }, [busy, onBusyChange]);

  const capes = canApply ? (current.data?.capes ?? []) : [];
  if (capes.length === 0) return null;

  // アカウントを切り替えた直後の前の表示は、触らせない
  const disabled = busy || blocked || current.isPlaceholderData;
  const pendingId = actions.setCape.isPending ? actions.setCape.variables?.capeId : undefined;
  const activeId = capes.find((c) => c.active)?.id ?? null;
  const choose = (capeId: string | null) => {
    if (capeId === activeId) return;
    actions.setCape.mutate({ capeId }, { onError: (e) => toast.showError(e) });
  };

  return (
    <div className="flex flex-wrap gap-3">
      {capes.map((cape) => (
        <CapeChip
          key={cape.id}
          label={cape.alias}
          selected={cape.id === activeId}
          pending={pendingId === cape.id}
          disabled={disabled}
          onPress={() => choose(cape.id)}
        >
          {cape.image && <CapeFace image={cape.image} />}
        </CapeChip>
      ))}
      <CapeChip
        label={t("skins.removeCape")}
        selected={activeId === null}
        pending={pendingId === null}
        disabled={disabled}
        onPress={() => choose(null)}
      >
        <IconNone aria-hidden className="size-6 text-text-muted" />
      </CapeChip>
    </div>
  );
}
