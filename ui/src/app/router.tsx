import { createHashRouter } from "react-router";
import { App } from "./App";
import { RouteError } from "./RouteError";
import Accounts from "../routes/Accounts";
import Browse from "../routes/Browse";
import Home from "../routes/Home";
import InstanceDetail from "../routes/InstanceDetail";
import InstanceModrinth from "../routes/InstanceModrinth";
import Instances from "../routes/Instances";
import Skins from "../routes/Skins";
import Settings from "../routes/Settings";
import SyncProfiles from "../routes/SyncProfiles";

export const router = createHashRouter([
  {
    path: "/",
    element: <App />,
    // 子の画面の失敗もここで受ける。アプリのバーごと置き換わる
    errorElement: <RouteError />,
    children: [
      { index: true, element: <Home /> },
      { path: "instances", element: <Instances /> },
      { path: "instances/:id", element: <InstanceDetail /> },
      { path: "instances/:id/modrinth", element: <InstanceModrinth /> },
      { path: "sync", element: <SyncProfiles /> },
      { path: "browse", element: <Browse /> },
      { path: "skins", element: <Skins /> },
      { path: "accounts", element: <Accounts /> },
      { path: "settings", element: <Settings /> },
    ],
  },
]);
