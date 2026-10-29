import { useEffect } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconRetry from "~icons/ph/arrow-clockwise";
import IconSpinner from "~icons/ph/circle-notch";
import { useDefaultSkins, useSkinActions } from "../../features/skins/useSkins";
import { useDelayedFlag } from "../ReadyGate";
import { Skeleton } from "../Skeleton";
import { useToast } from "../toast/useToast";
import { SkinCard } from "./SkinCard";

// 保存したスキンの格子と同じ並び・大きさ
const grid = "grid grid-cols-[repeat(auto-fill,minmax(104px,1fr))] gap-3";
const cardBox = "relative aspect-[2/3] w-full";
// 読み込み中に場所取りをするカードの数（取得元の種類数と同じ程度）
const SKELETON_COUNT = 9;

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

/** 取得に失敗したときの 1 枚。スキンのカードと同じ大きさで、文は出さない */
function RetryCard({ fetching, onPress }: { fetching: boolean; onPress: () => void }) {
  const { t } = useTranslation();
  return (
    <div aria-busy={fetching || undefined} className={cardBox}>
      <Button
        aria-label={t("create.retry")}
        isDisabled={fetching}
        onPress={onPress}
        className={[
          "absolute inset-0 flex items-center justify-center rounded-lg border-2 border-line bg-surface-1",
          "text-text-muted transition-colors duration-(--duration-fast) ease-out-quint",
          "hover:border-line-strong hover:bg-surface-2 pressed:bg-surface-3",
          ring,
        ].join(" ")}
      >
        <IconRetry aria-hidden className="size-6" />
      </Button>
      {fetching && (
        <IconSpinner
          aria-hidden
          data-testid="default-skins-retry-progress"
          className="pointer-events-none absolute top-1.5 right-1.5 size-5 text-text-muted motion-safe:animate-spin"
        />
      )}
    </div>
  );
}

/**
 * デフォルトのスキンの格子。取得元が出す全種類を並べ、押すと適用する（ライブラリには足さない）。
 * 取得元のスキンには画像の sha256 がなく、着ているスキンと突き合わせられないので、チェックは付けない。
 * 取れなかったときは再試行のカード 1 枚だけを出す（失敗の通知は出さない。カードが知らせる）。
 * blocked は保存したスキン側の適用・追加などの最中。
 */
export function DefaultSkins({ accountId, canApply, blocked = false, onBusyChange }: {
  accountId: string | null;
  canApply: boolean;
  blocked?: boolean;
  /** 適用の最中かどうかを親へ伝える（アカウントの切り替えや保存したスキンを止めるのに使う） */
  onBusyChange?: (busy: boolean) => void;
}) {
  const query = useDefaultSkins();
  const actions = useSkinActions(accountId);
  const toast = useToast();
  // 150 ms 以内に返る取得では場所取りを出さない
  const showSkeleton = useDelayedFlag(query.isPending);

  const busy = actions.busy;
  useEffect(() => {
    onBusyChange?.(busy);
    return () => onBusyChange?.(false);
  }, [busy, onBusyChange]);

  // 再試行の最中は status が pending に戻るので、一度でも失敗してデータがない間は再試行のカードのままにする
  if (!query.data && query.errorUpdateCount > 0) {
    return (
      <div className={grid}>
        <RetryCard fetching={query.isFetching} onPress={() => void query.refetch()} />
      </div>
    );
  }
  if (!query.data) {
    return (
      <div className={grid}>
        {showSkeleton &&
          Array.from({ length: SKELETON_COUNT }, (_, i) => (
            <div key={i} data-testid="default-skin-skeleton" className={cardBox}>
              <Skeleton className="h-full w-full rounded-lg" />
            </div>
          ))}
      </div>
    );
  }

  const applying = actions.applyDefault.isPending ? actions.applyDefault.variables?.key : undefined;
  return (
    <div className={grid}>
      {query.data.map((skin) => (
        <SkinCard
          key={skin.key}
          image={skin.image}
          name={skin.name}
          variant={skin.variant}
          selected={false}
          pending={applying === skin.key}
          disabled={!canApply || busy || blocked}
          onPress={() => actions.applyDefault.mutate({ key: skin.key }, { onError: (e) => toast.showError(e) })}
        />
      ))}
    </div>
  );
}
