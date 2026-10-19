import {
	Badge,
	Button,
	Divider,
	FileButton,
	Group,
	NumberInput,
	Paper,
	Select,
	SimpleGrid,
	Stack,
	Switch,
	TagsInput,
	Text,
	Textarea,
	Title,
} from "@mantine/core";
import {
	CookieIcon,
	FloppyDiskIcon,
	HardDrivesIcon,
	SubtitlesIcon,
	TrashIcon,
	UploadSimpleIcon,
	VideoCameraIcon,
} from "@phosphor-icons/react";
import { createFileRoute } from "@tanstack/react-router";
import { type FormEvent, useState } from "react";
import { adminClient } from "../admin/api";
import { useAdmin } from "../admin/context";
import { PageHeading } from "../admin/shared";
import type { ApplicationSettings } from "../gen/proto/admin_pb";

export const Route = createFileRoute("/admin/settings")({
	component: Settings,
});

function Settings() {
	const { data } = useAdmin();
	return data?.settings ? <SettingsForm initial={data.settings} /> : null;
}

function SettingsForm({ initial }: { initial: ApplicationSettings }) {
	const { run } = useAdmin();
	const [baseline, setBaseline] = useState(initial);
	const [draft, setDraft] = useState(initial);
	const [saving, setSaving] = useState(false);
	const dirty = JSON.stringify(draft) !== JSON.stringify(baseline);
	const hasCookie = Boolean(draft.courseraCookie?.trim());
	const submit = async (event: FormEvent) => {
		event.preventDefault();
		setSaving(true);
		const saved = await run(
			() => adminClient.saveSettings({ settings: draft }),
			"Configuration saved.",
		);
		if (saved) setBaseline(draft);
		setSaving(false);
	};
	return (
		<>
			<PageHeading title="Configuration" />
			<form onSubmit={submit}>
				<Stack gap="md">
					<Paper withBorder className="settings-section">
						<div className="settings-intro">
							<CookieIcon size={18} weight="duotone" />
							<Title order={3}>Coursera session (cookies.txt)</Title>
							<Badge
								size="xs"
								variant="light"
								color={hasCookie ? "teal" : "gray"}
								ml="auto"
							>
								{hasCookie ? "Configured" : "Not set"}
							</Badge>
						</div>
						<Stack className="settings-fields" gap="sm">
							<Group
								justify="space-between"
								align="center"
								wrap="wrap"
								gap="xs"
							>
								<Text size="xs" c="dimmed">
									Paste your Coursera cookies (Netscape cookies.txt,
									Cookie-Editor JSON, or raw Cookie header) or upload a file.
								</Text>
								<Group gap="xs">
									<FileButton
										accept=".txt,.json,text/plain,application/json"
										onChange={async (file) => {
											if (!file) return;
											const content = await file.text();
											setDraft({ ...draft, courseraCookie: content });
										}}
									>
										{(props) => (
											<Button
												{...props}
												size="compact-xs"
												variant="default"
												leftSection={<UploadSimpleIcon size={13} />}
											>
												Upload cookies.txt
											</Button>
										)}
									</FileButton>
									{hasCookie && (
										<Button
											size="compact-xs"
											variant="subtle"
											color="red"
											leftSection={<TrashIcon size={13} />}
											onClick={() => setDraft({ ...draft, courseraCookie: "" })}
										>
											Clear
										</Button>
									)}
								</Group>
							</Group>
							<Textarea
								aria-label="cookies.txt content"
								placeholder="# Netscape HTTP Cookie File, Cookie-Editor JSON array, or CAUTH=..."
								value={draft.courseraCookie}
								onChange={(e) =>
									setDraft({
										...draft,
										courseraCookie: e.currentTarget.value,
									})
								}
								autosize
								minRows={6}
								maxRows={14}
								styles={{
									input: {
										fontFamily: "var(--font-mono)",
										fontSize: "0.75rem",
									},
								}}
							/>
						</Stack>
					</Paper>

					<Paper withBorder className="settings-section">
						<div className="settings-intro">
							<VideoCameraIcon size={18} weight="duotone" />
							<Title order={3}>Video</Title>
						</div>
						<Stack className="settings-fields" gap="md">
							<SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
								<Select
									label="Preferred quality"
									value={String(draft.preferredVideoHeight)}
									allowDeselect={false}
									data={[240, 360, 540, 720, 1080].map((v) => ({
										value: String(v),
										label: `${v}p${v === 720 ? " · HD" : v === 1080 ? " · Full HD" : ""}`,
									}))}
									onChange={(v) => {
										if (v)
											setDraft({ ...draft, preferredVideoHeight: Number(v) });
									}}
								/>
								<Select
									label="Video format"
									value={draft.preferredVideoFormat}
									allowDeselect={false}
									data={[
										{ value: "mp4", label: "MP4" },
										{ value: "webm", label: "WebM" },
									]}
									onChange={(v) => {
										if (v) setDraft({ ...draft, preferredVideoFormat: v });
									}}
								/>
							</SimpleGrid>
							<Select
								label="Fallback policy"
								value={draft.qualityFallbackPolicy}
								allowDeselect={false}
								data={[
									{
										value: "best_at_or_below",
										label: "Best quality at or below preference",
									},
									{
										value: "exact",
										label: "Exact quality only",
									},
								]}
								onChange={(v) => {
									if (v) setDraft({ ...draft, qualityFallbackPolicy: v });
								}}
							/>
						</Stack>
					</Paper>

					<Paper withBorder className="settings-section">
						<div className="settings-intro">
							<SubtitlesIcon size={18} weight="duotone" />
							<Title order={3}>Subtitles & resources</Title>
						</div>
						<Stack className="settings-fields" gap="md">
							<Switch
								label="Download subtitles"
								checked={draft.downloadSubtitles}
								onChange={(e) =>
									setDraft({
										...draft,
										downloadSubtitles: e.currentTarget.checked,
									})
								}
							/>
							<TagsInput
								label="Subtitle languages"
								placeholder="en, es, fr"
								value={draft.subtitleLanguages}
								disabled={!draft.downloadSubtitles}
								onChange={(v) => setDraft({ ...draft, subtitleLanguages: v })}
							/>
							<Divider />
							<Switch
								label="Download supplementary resources"
								checked={draft.downloadSupplements}
								onChange={(e) =>
									setDraft({
										...draft,
										downloadSupplements: e.currentTarget.checked,
									})
								}
							/>
						</Stack>
					</Paper>

					<Paper withBorder className="settings-section">
						<div className="settings-intro">
							<HardDrivesIcon size={18} weight="duotone" />
							<Title order={3}>Downloads</Title>
						</div>
						<Stack className="settings-fields" gap="md">
							<NumberInput
								label="Concurrent downloads"
								min={1}
								max={32}
								allowDecimal={false}
								allowNegative={false}
								required
								value={draft.maxConcurrentDownloads}
								onChange={(v) =>
									setDraft({
										...draft,
										maxConcurrentDownloads: typeof v === "number" ? v : 0,
									})
								}
							/>
						</Stack>
					</Paper>

					<Group justify="flex-end" gap="xs" className="settings-save">
						<Button
							size="xs"
							variant="default"
							disabled={!dirty || saving}
							onClick={() => setDraft(baseline)}
						>
							Discard
						</Button>
						<Button
							size="xs"
							type="submit"
							loading={saving}
							disabled={!dirty || draft.maxConcurrentDownloads < 1}
							leftSection={<FloppyDiskIcon size={15} />}
						>
							Save changes
						</Button>
					</Group>
				</Stack>
			</form>
		</>
	);
}
