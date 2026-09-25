<script setup lang="ts">
import { Plus, Trash2 } from 'lucide-vue-next'
import type {
  ContactInformationInput,
  EditorialAction,
  GeneralInformationTypeFields,
  AssetVersionReference,
  MediaUseReference,
  ProductCategoryPresentationInput,
  ProductFamily,
  SocialLinkInput,
} from '@airtek/contracts'
import EditorialActionField from '../fields/EditorialActionField.vue'
import MediaAssetField from '../fields/MediaAssetField.vue'

const props = defineProps<{ modelValue: GeneralInformationTypeFields }>()
const emit = defineEmits<{ 'update:modelValue': [value: GeneralInformationTypeFields] }>()

const FAMILIES: ProductFamily[] = ['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors']
const CONTACT_FIELDS = [
  ['email', '联系邮箱', 'email'],
  ['phone', '联系电话', 'tel'],
  ['locality', '城市', 'text'],
  ['region', '省/地区', 'text'],
  ['postalCode', '邮政编码', 'text'],
  ['countryCode', '国家代码', 'text'],
] as const

function patch(values: Partial<GeneralInformationTypeFields>): void {
  emit('update:modelValue', { ...props.modelValue, ...values })
}

function patchContact(values: Partial<ContactInformationInput>): void {
  patch({ contact: { ...props.modelValue.contact, ...values } })
}

function updateAddressLine(index: number, value: string): void {
  const addressLines = [...props.modelValue.contact.addressLines]
  addressLines[index] = value
  patchContact({ addressLines })
}

function addAddressLine(): void {
  patchContact({ addressLines: [...props.modelValue.contact.addressLines, ''] })
}

function removeAddressLine(index: number): void {
  patchContact({ addressLines: props.modelValue.contact.addressLines.filter((_, position) => position !== index) })
}

function updateSocial(index: number, values: Partial<SocialLinkInput>): void {
  const socialLinks = props.modelValue.socialLinks.map((link, position) => (
    position === index ? { ...link, ...values } : link
  ))
  patch({ socialLinks })
}

function addSocial(): void {
  patch({ socialLinks: [...props.modelValue.socialLinks, { service: '', url: '' }] })
}

function removeSocial(index: number): void {
  patch({ socialLinks: props.modelValue.socialLinks.filter((_, position) => position !== index) })
}

function updateCategory(index: number, values: Partial<ProductCategoryPresentationInput>): void {
  const productCategories = props.modelValue.productCategories.map((category, position) => (
    position === index ? { ...category, ...values } : category
  ))
  patch({ productCategories })
}

function addCategory(): void {
  patch({
    productCategories: [...props.modelValue.productCategories, {
      code: 'centrifugal',
      name: '',
      slug: '',
      description: '',
      sortOrder: props.modelValue.productCategories.length,
    }],
  })
}

function removeCategory(index: number): void {
  patch({ productCategories: props.modelValue.productCategories.filter((_, position) => position !== index) })
}

function updateDefaultSeo(values: Partial<GeneralInformationTypeFields['defaultSeo']>): void {
  patch({ defaultSeo: { ...props.modelValue.defaultSeo, ...values } })
}

function updateNavigationCta(action: EditorialAction | null): void {
  patch({ navigationCta: action })
}

function updateSiteIcon(value: AssetVersionReference | null): void {
  patch({ siteIcon: value })
}
</script>

<template>
  <div class="site-fields">
    <p class="inspector-copy">
      站点级配置不参与页面组成；这些字段直接驱动公共站的品牌、页脚与默认 SEO。
    </p>

    <section class="site-fields__group" aria-labelledby="gi-identity">
      <h3 id="gi-identity">品牌标识</h3>
      <label class="field">
        <span>组织名称</span>
        <input :value="modelValue.organizationName ?? ''" maxlength="200" placeholder="AIRTEKPOWER" @input="patch({ organizationName: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <label class="field">
        <span>品牌标语</span>
        <input :value="modelValue.brandLine ?? ''" maxlength="200" @input="patch({ brandLine: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <div class="field">
        <span>站点图标<small>须为至少 512 × 512 的正方形图片，并在发布前完成公开使用审核</small></span>
        <MediaAssetField
          :model-value="modelValue.siteIcon ?? null"
          mode="asset"
          usage="siteIcon"
          label="站点图标"
          @update:model-value="updateSiteIcon($event as AssetVersionReference | null)"
        />
        <small>发布此配置表示确认所选精确文件及版本拥有小尺寸公开使用授权；未设置时公开站使用中性占位图标。</small>
        <p v-if="!modelValue.siteIcon" class="empty-mini" role="status">
          发布就绪警告：尚未配置自定义站点图标；这不会阻断发布，但公开站将继续使用中性占位图标。
        </p>
      </div>
      <label class="field">
        <span>首页路径<small>必须是站内绝对路径，例如 /en</small></span>
        <input :value="modelValue.homePath ?? ''" placeholder="/en" @input="patch({ homePath: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <label class="field">
        <span>页脚声明</span>
        <textarea :value="modelValue.footerStatement ?? ''" rows="2" maxlength="500" @input="patch({ footerStatement: ($event.target as HTMLTextAreaElement).value || null })" />
      </label>
      <label class="field">
        <span>版权模板<small>可使用 {year} 占位符</small></span>
        <input :value="modelValue.copyrightTemplate ?? ''" maxlength="200" placeholder="© {year} AIRTEKPOWER" @input="patch({ copyrightTemplate: ($event.target as HTMLInputElement).value || null })" />
      </label>
    </section>

    <section class="site-fields__group" aria-labelledby="gi-contact">
      <h3 id="gi-contact">联系方式</h3>
      <label v-for="[key, label, type] in CONTACT_FIELDS" :key="key" class="field">
        <span>{{ label }}</span>
        <input
          :type="type"
          :value="modelValue.contact[key] ?? ''"
          @input="patchContact({ [key]: ($event.target as HTMLInputElement).value || null } as Partial<ContactInformationInput>)"
        />
      </label>
      <div class="field">
        <span>地址行</span>
        <div v-for="(line, index) in modelValue.contact.addressLines" :key="`address-${index}`" class="site-fields__repeat">
          <input :value="line" maxlength="200" :aria-label="`地址行 ${index + 1}`" @input="updateAddressLine(index, ($event.target as HTMLInputElement).value)" />
          <button class="icon-button" type="button" :aria-label="`删除地址行 ${index + 1}`" @click="removeAddressLine(index)"><Trash2 :size="15" /></button>
        </div>
        <button class="button button--quiet" type="button" @click="addAddressLine"><Plus :size="14" />添加地址行</button>
      </div>
    </section>

    <section class="site-fields__group" aria-labelledby="gi-social">
      <h3 id="gi-social">社交链接</h3>
      <p v-if="!modelValue.socialLinks.length" class="empty-mini">还没有社交链接。</p>
      <div v-for="(link, index) in modelValue.socialLinks" :key="`social-${index}`" class="site-fields__card">
        <label class="field">
          <span>服务名称</span>
          <input :value="link.service" maxlength="80" placeholder="linkedin" :aria-label="`社交服务 ${index + 1}`" @input="updateSocial(index, { service: ($event.target as HTMLInputElement).value })" />
        </label>
        <label class="field">
          <span>URL</span>
          <input :value="link.url" type="url" placeholder="https://" :aria-label="`社交链接 ${index + 1}`" @input="updateSocial(index, { url: ($event.target as HTMLInputElement).value })" />
        </label>
        <button class="button button--quiet" type="button" @click="removeSocial(index)"><Trash2 :size="14" />删除</button>
      </div>
      <button class="button button--quiet" type="button" @click="addSocial"><Plus :size="14" />添加社交链接</button>
    </section>

    <section class="site-fields__group" aria-labelledby="gi-seo">
      <h3 id="gi-seo">默认 SEO</h3>
      <label class="field">
        <span>默认标题<small>{{ (modelValue.defaultSeo.title ?? '').length }} / 60</small></span>
        <input :value="modelValue.defaultSeo.title ?? ''" maxlength="120" @input="updateDefaultSeo({ title: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <label class="field">
        <span>默认描述<small>{{ (modelValue.defaultSeo.description ?? '').length }} / 160</small></span>
        <textarea :value="modelValue.defaultSeo.description ?? ''" rows="2" maxlength="300" @input="updateDefaultSeo({ description: ($event.target as HTMLTextAreaElement).value || null })" />
      </label>
      <label class="toggle-row">
        <span><strong>允许索引</strong><small>站点级默认，可被单条内容覆盖</small></span>
        <input type="checkbox" :checked="modelValue.defaultSeo.indexable" @change="updateDefaultSeo({ indexable: ($event.target as HTMLInputElement).checked })" />
      </label>
      <div class="field">
        <span>默认社交分享图</span>
        <MediaAssetField
          :model-value="modelValue.defaultSeo.socialImage ?? null"
          mode="media"
          label="社交分享图"
          @update:model-value="updateDefaultSeo({ socialImage: $event as MediaUseReference | null })"
        />
      </div>
    </section>

    <section class="site-fields__group" aria-labelledby="gi-categories">
      <h3 id="gi-categories">产品分类呈现</h3>
      <p v-if="!modelValue.productCategories.length" class="empty-mini">还没有产品分类呈现。</p>
      <div v-for="(category, index) in modelValue.productCategories" :key="`category-${index}`" class="site-fields__card">
        <label class="field">
          <span>产品族</span>
          <select :value="category.code" :aria-label="`产品族 ${index + 1}`" @change="updateCategory(index, { code: ($event.target as HTMLSelectElement).value as ProductFamily })">
            <option v-for="family in FAMILIES" :key="family" :value="family">{{ family }}</option>
          </select>
        </label>
        <label class="field"><span>名称</span><input :value="category.name" :aria-label="`分类名称 ${index + 1}`" @input="updateCategory(index, { name: ($event.target as HTMLInputElement).value })" /></label>
        <label class="field"><span>Slug</span><input :value="category.slug" :aria-label="`分类 slug ${index + 1}`" @input="updateCategory(index, { slug: ($event.target as HTMLInputElement).value })" /></label>
        <label class="field"><span>描述</span><textarea :value="category.description" rows="2" :aria-label="`分类描述 ${index + 1}`" @input="updateCategory(index, { description: ($event.target as HTMLTextAreaElement).value })" /></label>
        <label class="field"><span>排序</span><input type="number" :value="category.sortOrder" :aria-label="`分类排序 ${index + 1}`" @input="updateCategory(index, { sortOrder: Number(($event.target as HTMLInputElement).value) || 0 })" /></label>
        <button class="button button--quiet" type="button" @click="removeCategory(index)"><Trash2 :size="14" />删除分类</button>
      </div>
      <button class="button button--quiet" type="button" @click="addCategory"><Plus :size="14" />添加产品分类</button>
    </section>

    <section class="site-fields__group" aria-labelledby="gi-cta">
      <h3 id="gi-cta">导航 CTA</h3>
      <EditorialActionField :model-value="modelValue.navigationCta ?? null" label="导航 CTA" @update:model-value="updateNavigationCta" />
    </section>
  </div>
</template>

<style scoped>
@layer components {
.site-fields { display: flex; flex-direction: column; gap: 1rem; }
.site-fields__group { display: flex; flex-direction: column; gap: 0.7rem; padding-top: 0.8rem; border-top: 1px solid var(--border-subtle); }
.site-fields__group h3 { margin: 0; color: var(--airtek-blue-dark); font-size: 0.75rem; letter-spacing: 0.06em; text-transform: uppercase; }
.site-fields__card { display: flex; flex-direction: column; gap: 0.45rem; padding: 0.6rem; border: 1px solid var(--border-default); border-radius: 9px; background: white; }
.site-fields__repeat { display: flex; align-items: center; gap: 0.3rem; margin-bottom: 0.3rem; }
.site-fields__repeat input { flex: 1; }
}
</style>
