import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { deleteScreenshot, importScreenshots, listScreenshots } from "../../api/commands";

const screenshotsKey = (instanceId: string) => ["screenshots", instanceId] as const;

/** ゲームの中で撮ったものも見えるよう、ウィンドウに戻ったときに取り直す（react-query の既定） */
export function useScreenshots(instanceId: string) {
  return useQuery({ queryKey: screenshotsKey(instanceId), queryFn: () => listScreenshots(instanceId) });
}

export function useScreenshotActions(instanceId: string) {
  const client = useQueryClient();
  const refresh = () => client.invalidateQueries({ queryKey: screenshotsKey(instanceId) });
  const remove = useMutation({
    mutationFn: ({ name }: { name: string }) => deleteScreenshot(instanceId, name),
    // 一覧が古くて既に無いときも、取り直して最新に合わせる
    onSettled: refresh,
  });
  const add = useMutation({
    mutationFn: ({ paths }: { paths: string[] }) => importScreenshots(instanceId, paths),
    onSuccess: refresh,
  });
  return { remove, add };
}
