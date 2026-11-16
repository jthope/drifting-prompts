import { Text, View } from "nsh";
import type { ProjectInfo } from "mdviewer/native";
import { EdgedScroll, IconButton, look, NavRow, NavSection, SidebarFrame } from "../../../dp/ui";
import { chooseFolder, openProject, projectMenu, showNew, showSettings } from "./actions";
import { page, projects } from "./model";

/**
 * The narrowest and widest the sidebar is dragged to, in points. Its width is
 * the user's, saved with the preferences (`sidebarWidth`, 232 at first).
 */
export const SIDEBAR_MIN_WIDTH = 180;
export const SIDEBAR_MAX_WIDTH = 400;

function ProjectRow(props: { project: ProjectInfo }) {
  return (
    <NavRow
      label={props.project.name}
      icon="folder"
      selected={page() === "project" && projects().selected === props.project.path}
      menu={() => projectMenu(props.project)}
      onPress={() => openProject(props.project.path)}
    />
  );
}

/**
 * The app's navigation, as IP Camera's: New, which opens a folder as a
 * project, then the projects, newest first, each by its folder's name, and
 * Settings pinned at the bottom, which opens in a dialog over the window. Only
 * the projects scroll, under their heading, which keeps its button in reach
 * (`EdgedScroll`).
 */
export function Sidebar(props: { onHide: () => void }) {
  return (
    <SidebarFrame product="mdviewer" name="Markdown Viewer" onHide={props.onHide}>
      <View class="gap-1 px-2 pb-1.5 pt-2">
        <NavRow label="New" icon="plus" selected={page() === "new"} onPress={showNew} />
        <NavSection title="Projects">
          <IconButton icon="folder-plus" label="Open a folder" onPress={chooseFolder} />
        </NavSection>
      </View>

      <EdgedScroll testID="sidebar-projects" place="sidebar" fade={look().tiers.frame} contentClass="gap-1 px-2 pb-2 pt-1">
        <Each of={projects().projects} key={(each: ProjectInfo) => each.path}>
          {(each) => <ProjectRow project={each()} />}
        </Each>
        <If when={projects().projects.length === 0}>
          <Text class="py-1.5 pl-2 text-xs text-muted-foreground">No projects yet</Text>
        </If>
      </EdgedScroll>

      {/* Over the project or New, which stays selected behind it. */}
      <View class="gap-1 p-2">
        <NavRow label="Settings" icon="settings" onPress={showSettings} />
      </View>
    </SidebarFrame>
  );
}
