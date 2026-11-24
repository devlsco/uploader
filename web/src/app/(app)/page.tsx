"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { PageHeader, useApp } from "@/components/app/app-shell";
import { Tour } from "@/components/app/tour";
import { UploadOptionsBar, loadOptions, saveOptions } from "@/components/app/upload-options";
import { Dropzone } from "@/components/upload/dropzone";
import { LinkForm } from "@/components/upload/link-form";
import { PasteForm } from "@/components/upload/paste-form";
import { Recent } from "@/components/upload/recent";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useParam } from "@/hooks/use-param";
import type { UploadOptions } from "@/lib/api";

export default function UploadPage() {
  const { user, instance } = useApp();
  const router = useRouter();
  const tourParam = useParam("tour");
  const [tourClosed, setTourClosed] = useState(false);
  const showTour = !tourClosed && (!user.onboarded || tourParam === "1");
  const [options, setOptions] = useState<UploadOptions>(() => (typeof window === "undefined" ? {} : loadOptions()));
  const [tab, setTab] = useState("file");
  const change = (o: UploadOptions) => {
    setOptions(o);
    saveOptions(o);
  };
  return (
    <div className="flex flex-col">
      <PageHeader title="Upload" description="The link is copied as soon as it is ready." />
      <Tabs value={tab} onValueChange={(v) => setTab(String(v))} className="gap-4">
        <div data-tour="tabs" className="w-fit">
          <TabsList>
            <TabsTrigger value="file">
              File
            </TabsTrigger>
            <TabsTrigger value="paste">
              Paste
            </TabsTrigger>
            <TabsTrigger value="link">
              Link
            </TabsTrigger>
          </TabsList>
        </div>
        <div data-tour="options" className="w-fit max-w-full">
          <UploadOptionsBar value={options} onChange={change} />
        </div>
        <TabsContent value="file">
          <Dropzone options={options} maxSize={instance.max_upload} chunkSize={instance.chunk_size} />
        </TabsContent>
        <TabsContent value="paste">
          <PasteForm options={options} />
        </TabsContent>
        <TabsContent value="link">
          <LinkForm base={instance.url} options={options} />
        </TabsContent>
      </Tabs>
      {tab === "file" && (
        <div className="mt-10">
          <Recent />
        </div>
      )}
      {/* the drop zone and the options are on the File tab */}
      {showTour && (
        <Tour
          name={user.username}
          onStep={(id) => {
            if (id === "dropzone" || id === "options") setTab("file");
          }}
          onClose={() => {
            setTourClosed(true);
            if (tourParam) router.replace("/");
          }}
        />
      )}
    </div>
  );
}
