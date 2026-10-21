import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addContentFiles,
  identifyContent,
  listContent,
  listContentCategories,
  removeContent,
  setContentEnabled,
} from "../../api/commands";
import type { ContentKind } from "../../bindings/ContentKind";

const contentKey = (instanceId: string) => ["content", instanceId] as const;

export function useContentList(instanceId: string, kind: ContentKind) {
  return useQuery({ queryKey: [...contentKey(instanceId), kind], queryFn: () => listContent(instanceId, kind) });
}

/**
 * 未特定の中身を特定する。タブを開いたときと、ファイルを追加したあとに呼ぶ。失敗は黙って無視する。
 * オフラインで失敗しても jar の中の名前やアイコンは書き終えているので、成否にかかわらず一覧を取り直す。
 */
export function useIdentifyContent(instanceId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => identifyContent(instanceId),
    onSettled: () => client.invalidateQueries({ queryKey: contentKey(instanceId) }),
    onError: () => {},
  });
}

export function useCategories(kind: ContentKind) {
  return useQuery({
    queryKey: ["content-categories", kind],
    queryFn: () => listContentCategories(kind),
    staleTime: Infinity,
  });
}

/** 有効、無効を切り替える。一覧は一部だけ変わっていることがあるので、失敗でも取り直す。 */
export function useSetEnabled(instanceId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ kind, relPath, enabled }: { kind: ContentKind; relPath: string; enabled: boolean }) =>
      setContentEnabled(instanceId, kind, relPath, enabled),
    onSettled: () => client.invalidateQueries({ queryKey: contentKey(instanceId) }),
  });
}

export function useRemove(instanceId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ kind, relPath }: { kind: ContentKind; relPath: string }) =>
      removeContent(instanceId, kind, relPath),
    onSettled: () => client.invalidateQueries({ queryKey: contentKey(instanceId) }),
  });
}

export function useAddFiles(instanceId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ kind, paths, overwrite }: { kind: ContentKind; paths: string[]; overwrite: boolean }) =>
      addContentFiles(instanceId, kind, paths, overwrite),
    onSettled: () => client.invalidateQueries({ queryKey: contentKey(instanceId) }),
  });
}
